# The offline build

This fork's playable build: the game without the launcher and without an account. A player
unpacks an archive from the repository's Releases, runs `play.bat` or `./play.sh`, chooses a
player name and plays. `packaging/offline/PLAY.txt` (and `PLAY.ru.txt`) is the note in the folder.

## Why there is no launcher in it

The Microsoft account is a requirement of the launcher, not of the game
([microsoft-account.md](microsoft-account.md)): every Play there goes through the owner gate, and
the sign-in runs under Horizon's own Azure app, which a fork may not use
([forking.md](forking.md#microsoft-sign-in-in-a-fork)). A launcher built from this repository has
no client ID, so it says that sign-in is not configured and starts nothing. The launcher also
installs upstream's signed game builds, not this repository's.

The game needs none of that. `horizon-client` takes a player name (`--name`,
`HORIZON_PLAYER_NAME`, else a generated one it keeps) and plays without a session: single player,
LAN, and servers with `online-mode=false`. A server that checks accounts refuses it, as vanilla's
does for a player who is not signed in. So the offline build ships the game and a start script,
and leaves the launcher and its gate as upstream wrote them: nothing in `launcher/` changed.

## What is in the folder

| File | What it is |
|---|---|
| `horizon-client` | The game: horizon-client's own executable, not `horizon-official`. The official server in the list, Discord presence, joining a Friends world, the "I'm playing" ping and the "Start Horizon from the launcher" screen are registered only by `horizon-official` ([forking.md](forking.md)), so none is in this build. |
| `horizon-ctl`, `horizon-prepare`, `horizon-server` | As in an upstream release folder. |
| `play.bat` + `play.ps1` (Windows), `play.sh` (Linux) | The start script, below. |
| `PLAY.txt`, `PLAY.ru.txt` | What the first run needs, in English and Russian. |
| `tools/`, `mods/horizon/src/test/java`, `crates/horizon-prepare`, `research-lock.json`, `LICENSE` | The sources a release folder takes (`ARCHIVED` in `xtask/src/main.rs`): horizon-prepare compiles its exporters from them. |
| `dxcompiler.dll` and its licences (Windows) | Microsoft's DXC, as upstream's Windows build (`tools/ci/fetch_dxc.py`). |

There is no `exporters/` bundle. Upstream compiles it on the maintainer's machine against his
Minecraft jars; a GitHub runner has none, and this build downloads none. Without the bundle
horizon-prepare compiles the exporters on the player's machine the first time it prepares
(`exporters::classes`), which is why the first run needs a Java 25 **JDK** and not only a runtime.

None of Minecraft's files are in the folder. The first run prepares them from the player's own
Minecraft: Java Edition 26.3, as every Horizon build does ([native-client.md](native-client.md)).

## The start script

`packaging/offline/play.sh` and `play.ps1` do the same things:

1. The player name: `HORIZON_PLAYER_NAME`, else the first line of `player-name.txt` beside the
   script, else it asks and keeps the answer there. 3 to 16 letters, digits or underscores.
2. `horizon-prepare --check`. When it reports a gap, the rest of this list, then
   `horizon-prepare --prepare --complete`.
3. The launcher folder to read Minecraft from, handed to horizon-prepare as
   `HORIZON_MINECRAFT_DIR`: the variable itself when the player set it, else the first line of
   `minecraft-dir.txt` beside the script, else, when the official launcher has no copy of the
   pinned version, the first of these that has one (below). Otherwise nothing: horizon-prepare
   looks in the official launcher's, ATLauncher's and the CurseForge app's folders itself.
4. The server jar: `HORIZON_SERVER_JAR`, else `server.jar` beside the script, else the script
   downloads the address the player's own `versions/26.3/26.3.json` names
   (`downloads.server.url`, a `mojang.com` address only); horizon-prepare checks its checksum
   against `research-lock.json` as it does for every file. A Prism-family folder has no such file
   and needs no server jar: horizon-prepare takes the server side from its client jar.
5. When preparing fails with nothing prepared, it stops with horizon-prepare's message, and
   names each Prism-family folder it found without the pinned version. When only something
   optional is missing (the sounds, say), it asks whether to start anyway.
6. `horizon-client --resources .local/horizon-client/26.3 --game-dir game --home game --name NAME`,
   with the script's own arguments after them. Saves, options, logs and crash reports stay in
   `game/` in the folder, apart from an official Horizon install's.

### PineconeMC and Prism Launcher

horizon-prepare reads a Prism Launcher or MultiMC root as a whole (`artifacts.rs`,
`prism_game_files`: the Minecraft component in `meta/net.minecraft/`, the client jar and the
libraries in `libraries/`, `assets/`), but only where it is told to look: in upstream the Horizon
launcher finds those folders for it (`launcher/src/sources.rs`, `known_folders`). The start
script stands in for that, for the launchers below. A root has the version when it holds
`meta/net.minecraft/<version>.json` and
`libraries/com/mojang/minecraft/<version>/minecraft-<version>-client.jar`, which the launcher
downloads the first time the player starts an instance of that version.

| Launcher | Windows | Linux |
|---|---|---|
| PineconeMC (ElyPrismLauncher) | `ELYPRISMLAUNCHER_DATA_DIR`, `%APPDATA%\ElyPrismLauncher`, `%APPDATA%\PineconeMC` | `ELYPRISMLAUNCHER_DATA_DIR`, `ElyPrismLauncher` and `PineconeMC` in `$XDG_DATA_HOME` (`~/.local/share`), the Flatpak's `~/.var/app/io.github.elyprismlauncher.ElyPrismLauncher/data/ElyPrismLauncher`, the snap's `~/snap/pineconemc/common` |
| Prism Launcher | `%APPDATA%\PrismLauncher` | `PrismLauncher` in `$XDG_DATA_HOME`, the Flatpak's `~/.var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher` |

PineconeMC is a fork of Prism Launcher with the same layout. Its data folder is named after the
project's first name, ElyPrismLauncher (`Launcher_CommonName` in its `program_info/CMakeLists.txt`),
and `ELYPRISMLAUNCHER_DATA_DIR` is the variable it reads itself; `PineconeMC` is looked in too
in case a later version renames the folder. Its own metadata server describes 26.3 with the
client jar and the asset index research-lock.json pins, so its files pass the same checks as the
official launcher's. A portable install keeps its data where it was unpacked (`UserData` there,
when it exists), which no script can guess: the player writes that folder into
`minecraft-dir.txt`. The Java such a launcher downloads for the game is a runtime without
`javac`, so the JDK is still needed.

## Building and publishing

`.github/workflows/offline-build.yml` builds Windows x64 and Linux x86_64 on GitHub's runners and
publishes the release `offline-v<version>`, where the version is `packaging/offline/VERSION`
(built in as `offline-<version>`). It runs when a push changes `packaging/offline/`,
`tools/package_offline.sh` or the workflow itself, on any branch, and by hand
(`workflow_dispatch`, once the workflow is on the default branch). A release of the same version
is replaced. To publish a new build, raise the number in `VERSION` and push.

It takes no secret and no self-hosted runner. Its checks are the ones a runner can make without
Minecraft: every program starts and reports the version, the programs import only system
libraries (`tools/check_imports.py`), the start script takes a name, runs horizon-prepare and
stops at its error, and it finds a stand-in PineconeMC folder and hands it to horizon-prepare. The
game is not played there, and no real launcher's files are read: upstream's smoke run (a new world, 1500 frames)
needs prepared resources, which only a machine with Minecraft has.

By hand, from a checkout:

```sh
cargo build --release --locked -p horizon-client --bin horizon-client \
  -p horizon-client-lib --bin horizon-ctl -p horizon-prepare --bin horizon-prepare \
  -p horizon-server --bin horizon-server
tools/package_offline.sh target/release dist/horizon-offline-linux-x86_64        # Windows: add .exe
```

No macOS build: upstream builds, signs and notarizes its own on the maintainer's Mac.
