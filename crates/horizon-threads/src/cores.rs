//! The core plan: which cores the game's threads run on.
//!
//! The game spreads its work over threads (the render thread, the server thread, the worker
//! pool of [`crate::jobs`], the tick's helper threads), and until this module only said how
//! urgent each is ([`crate::favour_current`], [`crate::behind_server`]). Where they ran was the
//! system's choice, and a worker could be on the very core a frame or a tick woke up for.
//!
//! The plan splits the machine in two. A few cores are kept for the threads a frame or a tick
//! waits for: the render thread and the server thread of a client, the server thread of a
//! dedicated server. Every helper thread is confined to the other cores as it starts. So the
//! kept cores are idle whenever a favoured thread wants one, and it never waits for a helper to
//! be moved out of its way.
//!
//! The favoured threads themselves are not pinned to one core each. On Linux a thread hands
//! its affinity to every thread it starts, and the render and server threads start many that
//! this crate never sees (the graphics driver's, the audio's, the tick's helper threads): all
//! of them would be squeezed onto the one core. They keep the whole machine instead, and the
//! system puts them on the cores nothing else may use.
//!
//! Only machines with cores to spare are planned ([`plan`]); the others, and the systems with
//! no call for it (macOS: its quality of service classes do this job), run as before.
//! `HORIZON_CORE_PLAN=off` turns it off anywhere.
use std::fmt;
use std::sync::OnceLock;

/// The CPUs a set can name: the size of Linux's `cpu_set_t`.
const CAPACITY: usize = 1024;
const WORDS: usize = CAPACITY / 64;

/// A set of logical CPUs, by the numbers the system gives them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CpuSet([u64; WORDS]);

impl CpuSet {
    pub const EMPTY: Self = Self([0; WORDS]);

    /// The CPUs whose bit is set in `mask`, CPU 0 the lowest: a Windows affinity mask.
    pub const fn from_mask(mask: u64) -> Self {
        let mut set = Self::EMPTY;
        set.0[0] = mask;
        set
    }

    /// The first 64 CPUs as a mask ([`CpuSet::from_mask`]'s).
    pub const fn mask(&self) -> u64 {
        self.0[0]
    }

    /// The kernel's list form, `0-3,8,10-11` (sysfs's `thread_siblings_list`). `None` for
    /// anything else, or a CPU number the set can't hold.
    pub fn parse_list(list: &str) -> Option<Self> {
        let mut set = Self::EMPTY;
        for part in list.trim().split(',').filter(|part| !part.is_empty()) {
            let (first, last) = part.split_once('-').unwrap_or((part, part));
            let (first, last): (usize, usize) = (first.parse().ok()?, last.parse().ok()?);
            if first > last || last >= CAPACITY {
                return None;
            }
            for cpu in first..=last {
                set.insert(cpu);
            }
        }
        Some(set)
    }

    /// Adds `cpu`. False, and nothing added, for a number the set can't hold.
    pub fn insert(&mut self, cpu: usize) -> bool {
        let Some(word) = self.0.get_mut(cpu / 64) else {
            return false;
        };
        *word |= 1u64 << (cpu % 64);
        true
    }

    pub fn contains(&self, cpu: usize) -> bool {
        self.0
            .get(cpu / 64)
            .is_some_and(|word| word & (1u64 << (cpu % 64)) != 0)
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(|word| word.count_ones() as usize).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&word| word == 0)
    }

    /// The lowest CPU of the set.
    pub fn first(&self) -> Option<usize> {
        self.iter().next()
    }

    /// The CPUs of the set, lowest first.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..CAPACITY).filter(|&cpu| self.contains(cpu))
    }

    /// The CPUs of either set.
    pub fn with(mut self, other: Self) -> Self {
        for (word, more) in self.0.iter_mut().zip(other.0) {
            *word |= more;
        }
        self
    }

    /// The CPUs of both sets.
    pub fn and(mut self, other: Self) -> Self {
        for (word, kept) in self.0.iter_mut().zip(other.0) {
            *word &= kept;
        }
        self
    }

    /// The set less `other`'s CPUs.
    pub fn without(mut self, other: Self) -> Self {
        for (word, less) in self.0.iter_mut().zip(other.0) {
            *word &= !less;
        }
        self
    }
}

impl Default for CpuSet {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// The kernel's list form: `0-3,8,10-11`.
impl fmt::Display for CpuSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut cpus = self.iter().peekable();
        let mut separator = "";
        while let Some(first) = cpus.next() {
            let mut last = first;
            while cpus.next_if_eq(&(last + 1)).is_some() {
                last += 1;
            }
            if last == first {
                write!(f, "{separator}{first}")?;
            } else {
                write!(f, "{separator}{first}-{last}")?;
            }
            separator = ",";
        }
        Ok(())
    }
}

impl fmt::Debug for CpuSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CpuSet({self})")
    }
}

/// One physical core the process may run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Core {
    /// Its logical CPUs: one, or its hardware threads.
    pub cpus: CpuSet,
    /// Its highest frequency, in kHz, where the system tells it: a hybrid CPU's performance
    /// cores have the highest. 0 where it doesn't, which makes every core as fast as the others.
    pub speed: u64,
}

/// The two parts of the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Kept for the threads a frame or a tick waits for: no helper thread runs there.
    pub reserved: CpuSet,
    /// Where the helper threads run.
    pub helpers: CpuSet,
}

impl Plan {
    /// Every CPU of the plan: where a favoured thread may run.
    pub fn all(&self) -> CpuSet {
        self.reserved.with(self.helpers)
    }
}

/// Fewer hardware threads than this and nothing is planned: the floor of the parallel tick's
/// Auto, and one under [`crate::favour_current`]'s.
const MIN_THREADS: usize = 4;
/// A core is kept for a favoured thread only on a machine with this many physical cores for
/// each of them: 6 for a client's two, 3 for a dedicated server's one. A kept core is idle
/// while its thread sleeps, which a single player's server thread does most of a tick; on a
/// smaller machine that is too large a share of it to leave idle.
const CORES_PER_FAVOURED: usize = 3;
/// From this many physical cores for each favoured thread (8 for a client, 4 for a dedicated
/// server), a kept core is kept whole: the helpers stay off its other hardware thread too,
/// which shares the core's speed with the first. Below, the helpers get that thread.
const CORES_PER_WHOLE: usize = 4;

/// The plan for a machine of `cores` (in the system's order) whose process may run `threads`
/// threads at once ([`crate::worker_count`]), keeping a core for each of `favoured` threads.
/// `None` when the machine is too small to keep any ([`CORES_PER_FAVOURED`]).
///
/// The kept cores are the first of the fastest ones (within 5% of the highest frequency, as
/// `performance_cores` tells them apart), so a hybrid CPU's efficiency cores go to the helpers.
pub fn plan(cores: &[Core], favoured: usize, threads: usize) -> Option<Plan> {
    if favoured == 0 || threads < MIN_THREADS || cores.len() < favoured * CORES_PER_FAVOURED {
        return None;
    }
    let top = cores.iter().map(|core| core.speed).max()?;
    let fast = |core: &Core| core.speed >= top - top / 20;
    let whole = cores.len() >= favoured * CORES_PER_WHOLE;
    let mut reserved = CpuSet::EMPTY;
    let kept = cores.iter().filter(|core| fast(core));
    for core in kept
        .chain(cores.iter().filter(|core| !fast(core)))
        .take(favoured)
    {
        if whole {
            reserved = reserved.with(core.cpus);
        } else if let Some(cpu) = core.cpus.first() {
            reserved.insert(cpu);
        }
    }
    let all = cores
        .iter()
        .fold(CpuSet::EMPTY, |all, core| all.with(core.cpus));
    let helpers = all.without(reserved);
    (!reserved.is_empty() && !helpers.is_empty()).then_some(Plan { reserved, helpers })
}

/// `HORIZON_CORE_PLAN`: `0`, `off` or `false` leaves every thread where the system puts it.
fn enabled(value: Option<&str>) -> bool {
    !matches!(value.map(str::trim), Some("0" | "off" | "false"))
}

/// The process's plan, once [`reserve`] made it.
static PLAN: OnceLock<Option<Plan>> = OnceLock::new();

/// Makes the process's plan, keeping a core for each of `favoured` threads: 2 for a client
/// (its render thread and its server thread), 1 for a dedicated server. Call it once as the
/// game starts, before its helper threads do: they then stay off the kept cores
/// ([`confine_helper`]), and the automatic size of the worker pool follows
/// ([`crate::default_workers`]). Later calls change nothing. The plan, or `None` where there
/// is none: `HORIZON_CORE_PLAN=off`, a machine too small ([`plan`]), a system without the call.
///
/// ```
/// if let Some(plan) = horizon_threads::cores::reserve(2) {
///     println!("frames and ticks keep CPUs {}, helpers run on {}", plan.reserved, plan.helpers);
/// }
/// ```
pub fn reserve(favoured: usize) -> Option<Plan> {
    *PLAN.get_or_init(|| {
        if !enabled(std::env::var("HORIZON_CORE_PLAN").ok().as_deref()) {
            return None;
        }
        plan(&topology()?, favoured, crate::worker_count())
    })
}

/// The process's plan: `None` until [`reserve`], and where it made none.
pub fn current() -> Option<Plan> {
    PLAN.get().copied().flatten()
}

/// How many CPUs the plan leaves the helper threads; `None` without a plan.
pub fn helper_cpus() -> Option<usize> {
    current().map(|plan| plan.helpers.len())
}

/// Called by a helper thread as it starts ([`crate::behind_server`] does): it stays off the
/// kept cores from now on, and so do the threads it starts. Nothing without a plan. Whether
/// the system took it.
pub fn confine_helper() -> bool {
    current().is_some_and(|plan| confine(&plan.helpers))
}

/// Called by a favoured thread as it starts ([`crate::favour_current`] does): it may run on
/// every core of the plan, the kept ones included, even when a helper thread started it and
/// handed it its own confinement. Nothing without a plan. Whether the system took it.
pub fn free_favoured() -> bool {
    current().is_some_and(|plan| confine(&plan.all()))
}

/// The CPUs the calling thread may run on.
#[cfg(target_os = "linux")]
fn allowed() -> Option<CpuSet> {
    // SAFETY: `cpu_set_t` is a plain array of bits; all zeroes is the empty set.
    let mut mask: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    // SAFETY: reads the calling thread's own mask (pid 0) into a local of the size passed.
    let read = unsafe { libc::sched_getaffinity(0, std::mem::size_of_val(&mask), &mut mask) };
    if read != 0 {
        return None;
    }
    let mut set = CpuSet::EMPTY;
    for cpu in 0..CAPACITY {
        // SAFETY: `cpu` is below `CPU_SETSIZE`, which `CAPACITY` equals.
        if unsafe { libc::CPU_ISSET(cpu, &mask) } {
            set.insert(cpu);
        }
    }
    Some(set)
}

/// The physical cores the process may run on, from sysfs: the CPUs that share a core
/// (`thread_siblings_list`) and its highest frequency (`cpuinfo_max_freq`). Where sysfs says
/// neither (some containers and virtual machines), every CPU is a core, all as fast.
#[cfg(target_os = "linux")]
fn topology() -> Option<Vec<Core>> {
    let allowed = allowed()?;
    let mut cores = Vec::new();
    let mut seen = CpuSet::EMPTY;
    for cpu in allowed.iter() {
        if seen.contains(cpu) {
            continue;
        }
        let read = |file: &str| {
            std::fs::read_to_string(format!("/sys/devices/system/cpu/cpu{cpu}/{file}")).ok()
        };
        let mut cpus = read("topology/thread_siblings_list")
            .and_then(|list| CpuSet::parse_list(&list))
            .unwrap_or(CpuSet::EMPTY)
            .and(allowed)
            .without(seen);
        cpus.insert(cpu);
        let speed = read("cpufreq/cpuinfo_max_freq")
            .and_then(|khz| khz.trim().parse().ok())
            .unwrap_or(0);
        seen = seen.with(cpus);
        cores.push(Core { cpus, speed });
    }
    Some(cores)
}

/// The calling thread runs on `cpus` only from now on, and so do the threads it starts.
#[cfg(target_os = "linux")]
fn confine(cpus: &CpuSet) -> bool {
    // SAFETY: `cpu_set_t` is a plain array of bits; all zeroes is the empty set.
    let mut mask: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    for cpu in cpus.iter() {
        // SAFETY: a `CpuSet` holds no CPU from `CAPACITY` up, which is `CPU_SETSIZE`.
        unsafe { libc::CPU_SET(cpu, &mut mask) };
    }
    // SAFETY: sets the calling thread's own mask (pid 0) from a local of the size passed.
    unsafe { libc::sched_setaffinity(0, std::mem::size_of_val(&mask), &mask) == 0 }
}

/// The physical cores the process may run on (`GetLogicalProcessorInformation`). Windows does
/// not tell their speeds here; a hybrid CPU's performance cores come first in its numbering,
/// which is the order [`plan`] keeps cores in. `None` on a machine with more than one
/// processor group (over 64 logical processors): an affinity mask covers one group only.
#[cfg(windows)]
fn topology() -> Option<Vec<Core>> {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformation, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
    };
    use windows_sys::Win32::System::Threading::{
        GetActiveProcessorGroupCount, GetCurrentProcess, GetProcessAffinityMask,
    };
    // SAFETY: no arguments.
    if unsafe { GetActiveProcessorGroupCount() } != 1 {
        return None;
    }
    let (mut process, mut system) = (0usize, 0usize);
    // SAFETY: GetCurrentProcess's pseudo handle needs no closing; both masks are live locals.
    if unsafe { GetProcessAffinityMask(GetCurrentProcess(), &mut process, &mut system) } == 0 {
        return None;
    }
    let allowed = CpuSet::from_mask(process as u64);
    let entry = std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
    let mut bytes = 0u32;
    // SAFETY: with no buffer the call only writes the size it needs to `bytes`.
    unsafe { GetLogicalProcessorInformation(std::ptr::null_mut(), &mut bytes) };
    let mut entries = vec![SYSTEM_LOGICAL_PROCESSOR_INFORMATION::default(); bytes as usize / entry];
    if entries.is_empty() {
        return None;
    }
    bytes = (entries.len() * entry) as u32;
    // SAFETY: `entries` holds `bytes` bytes of entries, which the call fills at most.
    if unsafe { GetLogicalProcessorInformation(entries.as_mut_ptr(), &mut bytes) } == 0 {
        return None;
    }
    entries.truncate(bytes as usize / entry);
    let cores = entries
        .iter()
        .filter(|entry| entry.Relationship == RelationProcessorCore)
        .map(|entry| CpuSet::from_mask(entry.ProcessorMask as u64).and(allowed))
        .filter(|cpus| !cpus.is_empty())
        .map(|cpus| Core { cpus, speed: 0 })
        .collect();
    Some(cores)
}

/// The calling thread runs on `cpus` only from now on.
#[cfg(windows)]
fn confine(cpus: &CpuSet) -> bool {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadAffinityMask};
    // SAFETY: GetCurrentThread's pseudo handle needs no closing.
    unsafe { SetThreadAffinityMask(GetCurrentThread(), cpus.mask() as usize) != 0 }
}

/// No plan elsewhere. macOS has no affinity call on Apple silicon; its quality of service
/// classes ([`crate::favour_current`], [`crate::far_terrain_current`]) keep the helpers off
/// the performance cores a frame needs.
#[cfg(not(any(target_os = "linux", windows)))]
fn topology() -> Option<Vec<Core>> {
    None
}

#[cfg(not(any(target_os = "linux", windows)))]
fn confine(_cpus: &CpuSet) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{Core, CpuSet, Plan, plan};

    fn set(list: &str) -> CpuSet {
        CpuSet::parse_list(list).unwrap()
    }

    /// `count` cores of one speed, each with the hardware threads `threads` names for it.
    fn machine(count: usize, speed: u64, threads: impl Fn(usize) -> Vec<usize>) -> Vec<Core> {
        (0..count)
            .map(|core| {
                let mut cpus = CpuSet::EMPTY;
                for cpu in threads(core) {
                    assert!(cpus.insert(cpu));
                }
                Core { cpus, speed }
            })
            .collect()
    }

    /// Linux's usual numbering: core `i` is CPUs `i` and `i + count`.
    fn linux(count: usize) -> Vec<Core> {
        machine(count, 0, |core| vec![core, core + count])
    }

    /// Windows' numbering: core `i` is CPUs `2i` and `2i + 1`.
    fn windows(count: usize) -> Vec<Core> {
        machine(count, 0, |core| vec![2 * core, 2 * core + 1])
    }

    #[test]
    fn a_cpu_list_reads_and_prints_as_the_kernel_writes_it() {
        let cpus = set("0-3,8,10-11\n");
        assert_eq!(cpus.iter().collect::<Vec<_>>(), [0, 1, 2, 3, 8, 10, 11]);
        assert_eq!(cpus.len(), 7);
        assert_eq!(cpus.to_string(), "0-3,8,10-11");
        assert_eq!(set("5").to_string(), "5");
        assert_eq!(set("63-64,1023").iter().collect::<Vec<_>>(), [63, 64, 1023]);
        assert!(set("").is_empty());
        assert_eq!(CpuSet::EMPTY.to_string(), "");
        for bad in ["a", "3-1", "0-1024", "1,,x", "-2"] {
            assert_eq!(CpuSet::parse_list(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_mask_is_the_first_64_cpus() {
        let cpus = CpuSet::from_mask(0b1010_0001);
        assert_eq!(cpus.iter().collect::<Vec<_>>(), [0, 5, 7]);
        assert_eq!(cpus.mask(), 0b1010_0001);
        assert_eq!(CpuSet::from_mask(u64::MAX).len(), 64);
        let mut high = CpuSet::EMPTY;
        assert!(high.insert(64));
        assert!(!high.insert(1024));
        assert_eq!(high.mask(), 0);
        assert_eq!(high.first(), Some(64));
    }

    /// 8 cores of 2 threads, a client: the first two cores whole, the helpers on the other six.
    #[test]
    fn a_large_machine_keeps_whole_cores() {
        assert_eq!(
            plan(&linux(8), 2, 16),
            Some(Plan {
                reserved: set("0-1,8-9"),
                helpers: set("2-7,10-15"),
            })
        );
        assert_eq!(
            plan(&windows(8), 2, 16),
            Some(Plan {
                reserved: set("0-3"),
                helpers: set("4-15"),
            })
        );
    }

    /// 6 cores of 2 threads, a client: one hardware thread of each of two different cores,
    /// the helpers on everything else, those cores' second threads included.
    #[test]
    fn a_middle_machine_keeps_one_thread_of_each_core() {
        assert_eq!(
            plan(&linux(6), 2, 12),
            Some(Plan {
                reserved: set("0-1"),
                helpers: set("2-11"),
            })
        );
        // Windows numbers a core's two threads one after the other: CPUs 0 and 1 are one core.
        assert_eq!(
            plan(&windows(6), 2, 12),
            Some(Plan {
                reserved: set("0,2"),
                helpers: set("1,3-11"),
            })
        );
    }

    /// A dedicated server keeps one core, whole from 4 cores up.
    #[test]
    fn a_server_keeps_one_core() {
        for (cores, reserved, helpers) in [(6, "0,6", "1-5,7-11"), (8, "0,8", "1-7,9-15")] {
            assert_eq!(
                plan(&linux(cores), 1, 2 * cores),
                Some(Plan {
                    reserved: set(reserved),
                    helpers: set(helpers),
                })
            );
        }
    }

    /// 4 cores: too few to keep two of them for a client; a dedicated server keeps one, whole.
    #[test]
    fn a_small_machine_keeps_a_core_only_for_a_server() {
        assert_eq!(plan(&linux(4), 2, 8), None);
        assert_eq!(plan(&linux(5), 2, 10), None);
        assert_eq!(
            plan(&linux(4), 1, 8),
            Some(Plan {
                reserved: set("0,4"),
                helpers: set("1-3,5-7"),
            })
        );
        // 3 cores without hardware threads, as many virtual machines show them: one CPU.
        let plain = machine(4, 0, |core| vec![core]);
        assert_eq!(
            plan(&plain[..3], 1, 4),
            Some(Plan {
                reserved: set("0"),
                helpers: set("1-2"),
            })
        );
        assert_eq!(plan(&linux(2), 1, 4), None);
    }

    /// Nothing is kept for no thread, or in a process that may run fewer than 4 threads at
    /// once (a container's quota), whatever the machine under it.
    #[test]
    fn nothing_is_kept_without_threads_to_spare() {
        assert_eq!(plan(&linux(16), 0, 32), None);
        assert_eq!(plan(&linux(16), 2, 3), None);
        assert_eq!(plan(&[], 1, 8), None);
    }

    /// A hybrid CPU whose efficiency cores come first (big.LITTLE's numbering): the kept
    /// cores are performance cores, and every efficiency core is the helpers'.
    #[test]
    fn the_kept_cores_are_the_fastest() {
        let mut cores = machine(4, 1_800_000, |core| vec![core]);
        cores.extend(machine(4, 3_000_000, |core| vec![core + 4]));
        // One of them a little slower than the fastest, as cores of one kind can be.
        cores[5].speed = 2_900_000;
        assert_eq!(
            plan(&cores, 2, 8),
            Some(Plan {
                reserved: set("4-5"),
                helpers: set("0-3,6-7"),
            })
        );
        // Intel's numbering: 2 performance cores of 2 threads, then 8 efficiency cores.
        let mut intel = machine(2, 5_000_000, |core| vec![2 * core, 2 * core + 1]);
        intel.extend(machine(8, 3_800_000, |core| vec![core + 4]));
        assert_eq!(
            plan(&intel, 2, 12),
            Some(Plan {
                reserved: set("0-3"),
                helpers: set("4-11"),
            })
        );
        // More favoured threads than fast cores: the rest come from the slower ones.
        assert_eq!(plan(&intel, 3, 12).unwrap().reserved, set("0,2,4"));
    }

    #[test]
    fn only_a_refusal_turns_the_plan_off() {
        for off in ["0", "off", "false", " off "] {
            assert!(!super::enabled(Some(off)), "{off}");
        }
        for on in [None, Some(""), Some("1"), Some("on"), Some("auto")] {
            assert!(super::enabled(on), "{on:?}");
        }
    }

    /// No test makes the process's plan, so every thread here is where the system put it.
    #[test]
    fn nothing_moves_without_a_plan() {
        assert_eq!(super::current(), None);
        assert_eq!(super::helper_cpus(), None);
        assert!(!std::thread::spawn(super::confine_helper).join().unwrap());
        assert!(!std::thread::spawn(super::free_favoured).join().unwrap());
    }

    /// Every CPU the process may run on is in exactly one core.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_cores_are_the_cpus_the_process_may_use() {
        let (allowed, cores) = std::thread::spawn(|| (super::allowed(), super::topology()))
            .join()
            .unwrap();
        let (allowed, cores) = (allowed.unwrap(), cores.unwrap());
        assert!(!allowed.is_empty());
        let mut seen = CpuSet::EMPTY;
        for core in &cores {
            assert!(!core.cpus.is_empty());
            assert!(core.cpus.and(seen).is_empty(), "{:?} twice", core.cpus);
            seen = seen.with(core.cpus);
        }
        assert_eq!(seen, allowed);
    }

    /// A confined thread runs on its CPUs only, the threads it starts too, and a favoured
    /// thread among them gets the whole plan back.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_confined_thread_keeps_to_its_cpus_until_it_is_freed() {
        std::thread::spawn(|| {
            let all = super::allowed().unwrap();
            let mut helpers = CpuSet::EMPTY;
            // The last CPU, so that with two or more the first is the one kept.
            helpers.insert(all.iter().last().unwrap());
            let plan = Plan {
                reserved: all.without(helpers),
                helpers,
            };
            assert!(super::confine(&plan.helpers));
            assert_eq!(super::allowed(), Some(helpers));
            std::thread::spawn(move || {
                assert_eq!(super::allowed(), Some(helpers));
                assert!(super::confine(&plan.all()));
                assert_eq!(super::allowed(), Some(all));
            })
            .join()
            .unwrap();
            // The thread that freed itself changed nothing for the one that started it.
            assert_eq!(super::allowed(), Some(helpers));
            // No CPU at all is refused, and leaves the thread where it was.
            assert!(!super::confine(&CpuSet::EMPTY));
            assert_eq!(super::allowed(), Some(helpers));
        })
        .join()
        .unwrap();
    }
}
