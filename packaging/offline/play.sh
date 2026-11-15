#!/usr/bin/env bash
# Starts Horizon from this folder: no launcher, no account. The first run prepares the game's
# resources from your own Minecraft: Java Edition (PLAY.txt), later runs start the game at once.
#
#   ./play.sh [options of horizon-client]
#
# HORIZON_PLAYER_NAME   the player name, instead of asking and keeping it in player-name.txt
# HORIZON_MINECRAFT_DIR the launcher folder to read Minecraft from, instead of the first line of
#                       minecraft-dir.txt and of the folders looked in below
# HORIZON_SERVER_JAR    the pinned server jar, when it is not server.jar in this folder
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
prepare="$here/horizon-prepare"
client="$here/horizon-client"
version=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$here/research-lock.json" | head -1)
resources="$here/.local/horizon-client/$version"
game="$here/game"

fail() {
  printf '\n%s\n' "$*" >&2
  exit 1
}

[[ -n "$version" ]] || fail "research-lock.json beside play.sh names no Minecraft version: unpack the archive again."

# The player name: Minecraft's rule for one, so every server takes it.
valid() { [[ "$1" =~ ^[A-Za-z0-9_]{3,16}$ ]]; }
name_file="$here/player-name.txt"
name=${HORIZON_PLAYER_NAME:-}
if [[ -z "$name" && -f "$name_file" ]]; then
  name=$(head -n 1 "$name_file" | tr -d '\r\n')
fi
while ! valid "$name"; do
  [[ -z "$name" ]] || echo "\"$name\" is not a player name: 3 to 16 letters, digits or underscores."
  read -r -p "Player name: " name || fail "No player name. Set HORIZON_PLAYER_NAME or write one into player-name.txt."
done
[[ -n "${HORIZON_PLAYER_NAME:-}" ]] || printf '%s\n' "$name" > "$name_file"

# Whether a PineconeMC, Prism Launcher or MultiMC root has the pinned version: its Minecraft
# component and the client jar, where horizon-prepare reads them (artifacts.rs, prism_game_files).
has_version() {
  [[ -f "$1/meta/net.minecraft/$version.json" \
    && -f "$1/libraries/com/mojang/minecraft/$version/minecraft-$version-client.jar" ]]
}

# The launcher folder horizon-prepare reads Minecraft from, where the player named none.
# horizon-prepare itself looks in the official launcher's, ATLauncher's and the CurseForge app's;
# the Horizon launcher finds the others for it, and this script stands in for that: PineconeMC
# (ElyPrismLauncher, whose data folder keeps that name) and Prism Launcher, when the official
# launcher has no copy of the version.
without=()
minecraft_dir() {
  [[ -z "${HORIZON_MINECRAFT_DIR:-}" ]] || return 0
  local chosen="$here/minecraft-dir.txt" dir
  if [[ -f "$chosen" ]]; then
    dir=$(head -n 1 "$chosen" | tr -d '\r\n')
    if [[ -n "$dir" ]]; then
      export HORIZON_MINECRAFT_DIR="$dir"
      echo "Reading Minecraft from $dir (minecraft-dir.txt)"
      return 0
    fi
  fi
  local official="$HOME/.minecraft/versions/$version/$version"
  [[ ! ( -f "$official.json" && -f "$official.jar" ) ]] || return 0
  local data=${XDG_DATA_HOME:-$HOME/.local/share} flatpak="$HOME/.var/app" launcher root
  while IFS='|' read -r launcher root; do
    [[ -n "$root" ]] || continue
    if has_version "$root"; then
      export HORIZON_MINECRAFT_DIR="$root"
      echo "Reading Minecraft $version from $launcher: $root"
      return 0
    fi
    [[ ! -d "$root/instances" ]] || without+=("$launcher ($root)")
  done <<FOLDERS
PineconeMC|${ELYPRISMLAUNCHER_DATA_DIR:-}
PineconeMC|$data/ElyPrismLauncher
PineconeMC|$data/PineconeMC
PineconeMC|$flatpak/io.github.elyprismlauncher.ElyPrismLauncher/data/ElyPrismLauncher
PineconeMC|$HOME/snap/pineconemc/common
Prism Launcher|$data/PrismLauncher
Prism Launcher|$flatpak/org.prismlauncher.PrismLauncher/data/PrismLauncher
FOLDERS
}

# The pinned server jar, where the player gave none: the address their own launcher's version
# file names (Mojang's public download). horizon-prepare checks its checksum before using it.
# PineconeMC, Prism Launcher and MultiMC keep no such file and need no server jar: horizon-prepare
# takes the server side from their client jar.
server_jar() {
  [[ -z "${HORIZON_SERVER_JAR:-}" ]] || return 0
  local jar="$here/server.jar"
  if [[ ! -f "$jar" ]]; then
    local minecraft=${HORIZON_MINECRAFT_DIR:-$HOME/.minecraft}
    local json="$minecraft/versions/$version/$version.json"
    [[ -f "$json" ]] || return 0
    local url
    url=$(grep -o 'https://[a-z0-9.-]*\.mojang\.com/[^"]*/server\.jar' "$json" | head -1 || true)
    [[ -n "$url" ]] || return 0
    echo "Downloading the Minecraft $version server jar from $url"
    if command -v curl > /dev/null; then
      curl --fail --location --silent --show-error --output "$jar.part" "$url" || { rm -f "$jar.part"; return 0; }
    elif command -v wget > /dev/null; then
      wget --quiet --output-document "$jar.part" "$url" || { rm -f "$jar.part"; return 0; }
    else
      return 0
    fi
    mv "$jar.part" "$jar"
  fi
  export HORIZON_SERVER_JAR="$jar"
}

echo "Checking the prepared resources"
if ! "$prepare" --check; then
  echo
  echo "Preparing Horizon from your Minecraft $version (a few minutes, once)"
  minecraft_dir
  server_jar
  if ! "$prepare" --prepare --complete; then
    if [[ ! -f "$resources/horizon-resources.json" ]]; then
      for launcher in ${without[@]+"${without[@]}"}; do
        printf '\n%s has no Minecraft %s yet: create a %s instance there and start it once, then run this again.\n' \
          "$launcher" "$version" "$version" >&2
      done
      fail "Preparing failed: the lines above say what is missing. PLAY.txt lists what the first run needs."
    fi
    echo
    read -r -p "The resources are incomplete (see above). Start the game anyway? [y/N] " answer || answer=n
    [[ "$answer" == [yY]* ]] || exit 1
  fi
fi

mkdir -p "$game"
exec "$client" --resources "$resources" --game-dir "$game" --home "$game" --name "$name" "$@"
