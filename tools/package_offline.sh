#!/usr/bin/env bash
# Assembles the offline build's folder (docs/offline-build.md): the game, horizon-ctl, horizon-prepare and
# horizon-server from BIN_DIR, the start scripts and notes of packaging/offline, and the sources
# horizon-prepare compiles its exporters from on the player's machine.
#
#   tools/package_offline.sh BIN_DIR OUT_DIR [.exe]
#
# Unlike `cargo xtask package` it needs no exporters/ bundle: that bundle is compiled against Minecraft's jars,
# which a build on a public runner does not have. Without it horizon-prepare compiles the exporters itself
# the first time it prepares (crates/horizon-prepare/src/exporters.rs, `classes`), which takes a JDK.
set -euo pipefail

[[ $# -ge 2 ]] || { echo "usage: tools/package_offline.sh BIN_DIR OUT_DIR [.exe]" >&2; exit 2; }
bin=$1
out=$2
exe=${3:-}
repo=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)

rm -rf "$out"
mkdir -p "$out"
# The sources a release folder takes: ARCHIVED in xtask/src/main.rs.
git -C "$repo" archive --format=tar HEAD \
  tools mods/horizon/src/test/java crates/horizon-prepare research-lock.json LICENSE | tar -x -C "$out"

for program in horizon-client horizon-ctl horizon-prepare horizon-server; do
  [[ -f "$bin/$program$exe" ]] || { echo "$bin/$program$exe is missing; build it first" >&2; exit 1; }
  cp "$bin/$program$exe" "$out/"
done

offline="$repo/packaging/offline"
cp "$offline/PLAY.txt" "$offline/PLAY.ru.txt" "$repo/THIRD_PARTY_NOTICES.md" "$out/"
if [[ "$exe" == .exe ]]; then
  cp "$offline/play.bat" "$offline/play.ps1" "$out/"
else
  cp "$offline/play.sh" "$out/"
  chmod +x "$out/play.sh" "$out"/horizon-*
fi
