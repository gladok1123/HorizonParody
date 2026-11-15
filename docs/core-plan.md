# The core plan: which cores the game's threads run on

Code: `crates/horizon-threads/src/cores.rs`. Switch: `HORIZON_CORE_PLAN=off`.

## What was there before

The game was already spread over threads, and none of this changes:

| Thread | Work |
|---|---|
| Render thread (the main thread of a client) | frames, input, the window |
| Server thread (single player's, a dedicated server's) | the 20 ticks a second |
| Worker pool, `horizon-worker-N` (`horizon_threads::jobs`), one per core but one | chunk generation, light, meshing, chunk encoding, far terrain |
| Tick helpers, `tick-helper-N` ([parallel tick](hosting.md#parallel-tick-parallel_tick), off by default) | the phases of a tick that give the same result in any order |
| Network, saving, sound, resource loading | one or a few threads each |

What decided who runs first was a priority only: the render and server threads ahead
(`favour_current`: macOS, Windows), the helpers behind (`behind_server`: 10 nice levels down on
Linux). Where each thread ran was left to the system. With one worker per core but one, a worker
could be on the very core a frame or a tick woke up for, and the frame or the tick waited for the
system to move one of them.

## What the plan adds

The machine is split in two when the game starts:

- **Kept cores**, for the threads a frame or a tick waits for: two for a client (its render
  thread and its server thread, or its network thread on someone else's server), one for a
  dedicated server (its server thread).
- **Helper cores**, all the others. Every helper thread is confined to them as it starts: the
  worker pool, the far terrain threads, the world loading threads, everything that calls
  `behind_server`, `far_terrain_current` or `idle_current`.

No helper thread can run on a kept core, so a kept core is free whenever a favoured thread wants
one. The automatic size of the worker pool (Graphics > Worker threads on Auto,
`horizon_threads::default_workers`) is never more than the helper CPUs.

The game logs the split once as it starts:

```
Horizon: frames and ticks keep CPUs 0-1,8-9, the helper threads run on CPUs 2-7,10-15
```

### The favoured threads are not pinned

The render and server threads keep the whole machine, the helper cores included; the system puts
them on the kept cores because nothing else may use those. They are not tied to one core each,
on purpose: on Linux a thread hands its affinity to every thread it starts, and these two start
many that Horizon never sees (the graphics driver's, the sound's, the tick helpers). All of them
would be squeezed onto that one core.

For the same reason `favour_current` gives a thread the whole machine back: a favoured thread
that a confined helper started would otherwise inherit the helper's cores.

Threads a tick waits for but that asked for no priority (the chunk encoders, the save's
compression threads, the tick helpers) are where the system puts them, as before: they may use
the kept cores, which is right, since the server thread is waiting for them.

## Which machines get a plan

`plan` in `cores.rs`, per favoured thread (2 for a client, 1 for a dedicated server):

| Physical cores the process may use | Client | Dedicated server |
|---|---|---|
| fewer than 3 per favoured thread | no plan (under 6 cores) | no plan (under 3 cores) |
| 3 per favoured thread | one hardware thread of each of 2 cores (6 or 7 cores) | one hardware thread of 1 core (3 cores) |
| 4 or more per favoured thread | 2 whole cores (from 8 cores) | 1 whole core (from 4 cores) |

- A kept core is idle while its thread sleeps, and single player's server thread sleeps most of
  each tick. On a small machine that is too much of it to leave idle, so nothing is planned and
  the game runs as before.
- "Whole" means the core's second hardware thread (Hyper-Threading, SMT) is kept too: the two
  share the core's speed. On the middle machines the helpers get that second thread.
- The kept cores are the first of the fastest cores, so a hybrid CPU's efficiency cores all go to
  the helpers. Linux tells the cores' speeds (`cpuinfo_max_freq`); Windows does not here, and the
  plan takes the first cores, which are the performance cores in Intel's numbering.
- Only the CPUs the process may use count (`taskset`, a container's cpuset), and nothing is
  planned for a process that may run fewer than 4 threads at once (a container's CPU quota,
  `HORIZON_WORKER_THREADS`).

Examples, as the tests in `cores.rs` pin them:

| Machine | Client | Dedicated server |
|---|---|---|
| 4 cores, 8 threads | no plan | kept `0,4`; helpers `1-3,5-7` |
| 6 cores, 12 threads | kept `0-1`; helpers `2-11` | kept `0,6`; helpers `1-5,7-11` |
| 8 cores, 16 threads | kept `0-1,8-9`; helpers `2-7,10-15` | kept `0,8`; helpers `1-7,9-15` |

(Linux's numbering, where core `i` of `n` is CPUs `i` and `i + n`.)

## Systems

| System | Plan |
|---|---|
| Linux | yes: `sched_setaffinity`, the cores from sysfs |
| Windows | yes, up to 64 logical processors: `SetThreadAffinityMask`, the cores from `GetLogicalProcessorInformation`. None on a machine with several processor groups |
| macOS | none: Apple silicon has no affinity call, and the quality of service classes the game already sets keep the helpers off the performance cores |
| Android, others | none |

## Turning it off

`HORIZON_CORE_PLAN=off` (or `0`, `false`) in the game's environment: every thread is where the
system puts it and the worker pool has its old size. For a dedicated server, set it in the
environment `horizon-server` starts in.

## What is and is not measured

Checked: the plan for each kind of machine and the confinement itself (`cargo test -p
horizon-threads cores`: on Linux a confined thread, and the threads it starts, really run on
their CPUs only, and a favoured thread gets the machine back).

Not measured yet: frame times and tick times with the plan against without. It trades a little
chunk throughput (fewer workers, a kept core idle part of the time) for frames and ticks that do
not wait for a core; how much of each on a given machine is for a benchmark to say
([benchmark.md](benchmark.md), `horizon-client --benchmark standard`, with and without
`HORIZON_CORE_PLAN=off`). The thresholds above are a first setting, not a tuned one.
