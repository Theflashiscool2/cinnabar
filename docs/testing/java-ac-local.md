# Optional Mac controlled-server debugging

The assistant has completed combined controlled-server acceptance. Repeating this run is optional; use [Hypixel candidate testing](java-hypixel-candidate.md) for the next owner test. Gameplay merges still require owner approval.

Candidates (each contains the approved handoff, neither contains the other lane):
- A: `origin/candidate/java-lane-a`, `e88830a4451b72db718b0f016ffa34feee777697`.
- C: `origin/candidate/java-lane-c`, `ea5ca9851758be8879cf415e6cb3cac0ef41ff3d`.

Use a controlled local Java 26.3/protocol777 server, not Hypixel. These checks do not establish iPhone/Hypixel acceptance. Keep the same checkout, `play` profile, Rust toolchain, working APFS cache paths and account profile. Do not clean Cargo, recreate caches, delete worktrees, or change RUSTFLAGS. There is no guaranteed cold-build time.

## 1. Mount and check existing storage

In Terminal:

```sh
cd ~
ls -ld /Volumes/ExternalSSD
hdiutil attach /Volumes/ExternalSSD/CinnabarBuild.dmg.sparsebundle
```

If already mounted, skip `attach`. If it reports no mountable filesystem, STOP: do not create cache directories at the missing mount point or continue compiling. Preserve the image and use the separate recovery procedure. Once mounted:

```sh
df -h / /Volumes/CinnabarBuild /Volumes/ExternalSSD
cd /Volumes/ExternalSSD/CinnabarBuild/cinnabar-iphone-test
git status --short --branch
git fetch origin feature/native-java-foundation candidate/java-lane-a candidate/java-lane-c
```

Keep existing CARGO_BUILD_BUILD_DIR, CARGO_TARGET_DIR and GOCACHE values; check each referenced volume is mounted. Retain at least6GiB free on the build volume before a build (more is better). Stop on a failed fetch or switch; preserve local changes.

## 2. Start one disposable local server

Reuse an existing pinned local fixture if available. Otherwise, in a separate Terminal, verify your existing JDK and server jar:

```sh
java -version
javac -version
ls -lh "$HOME/Downloads/server-26.3.jar"
shasum -a 256 "$HOME/Downloads/server-26.3.jar"
```

The jar's expected SHA-256 is `d052f14d7a173734fba553711e5b570162e2f2a313267ee31a21b975a679be64`. Prefer the repository's verified manifest as authority: `crates/java-protocol/data/target.json`. Use the existing working JDK25 JAVA_HOME/PATH; do not use the macOS stub if no JDK is registered.

Create a NEW directory, never your normal world. Read https://aka.ms/MinecraftEULA before accepting it:

```sh
LOCAL_SERVER_DIR=$(mktemp -d "$HOME/cinnabar-lane-server.XXXXXX")
cd "$LOCAL_SERVER_DIR"
printf 'eula=true\n' > eula.txt
printf 'server-ip=127.0.0.1\nserver-port=25586\nonline-mode=false\naccepts-transfers=true\nspawn-protection=0\nview-distance=3\nsimulation-distance=3\nmax-players=2\nlevel-type=minecraft:flat\n' > server.properties
java -Xmx1G -jar "$HOME/Downloads/server-26.3.jar" nogui
```

Do not enable `allow-flight`; default native floating checks stay on. This is a loopback offline TEST server, not an authentication test. Keep this console open. Join using your existing selected Java account; no token/keychain export is needed. The commands below use selectors because this server has only your test player.

After the server reports ready, enter in its console:

```text
forceload add 96 96 127 127
fill 96 64 96 127 75 127 minecraft:air
fill 96 68 96 127 68 127 minecraft:stone
setworldspawn 104 69 102
setblock 114 68 110 minecraft:honey_block
setblock 116 69 110 minecraft:cobweb
fill 118 68 109 120 68 111 minecraft:slime_block
setblock 100 69 110 minecraft:stone_slab[type=bottom]
setblock 102 69 110 minecraft:stone_stairs[facing=north]
setblock 104 69 110 minecraft:oak_fence
setblock 106 69 110 minecraft:oak_trapdoor[half=bottom]
time set day
weather clear
```

## 3. Build/run A once, retaining the cache

From the repository Terminal:

```sh
set -o pipefail
git switch --detach origin/candidate/java-lane-a
git rev-parse HEAD
export CARGO_BUILD_JOBS=2
export CARGO_INCREMENTAL=1
export CINNABAR_JAVA_ASSETS="$PWD/.local/assets/java/26.3"
export CINNABAR_JAVA_TELEMETRY=1
export CINNABAR_SLOW_FRAME_LOG=0
export CINNABAR_LOG='warn,java_session=debug,java_auth=info,client_shell::menu=info'
TEST_DIR=$(mktemp -d "$HOME/Desktop/cinnabar-lane-a.XXXXXX")
touch "$TEST_DIR/started"
make play 2>&1 | tee "$TEST_DIR/console.log"
```

Add/select Java server `127.0.0.1:25586`; use your existing Java account. Wait for world/HUD readiness. Never use these unfinished movement checks on Hypixel.

1. Console: `gamemode survival @a`, then `tp @a 104.5 69 102.5 0 20.5`. Walk, sprint, jump/land, crouch-walk/release, and walk around partial blocks. Record jitter and unexpected corrections. Console `data get entity @p Pos` and `data get entity @p OnGround` after settling.
2. Test honey contact, cobweb slowdown and slime bounce; verify slowdown/stickiness does not persist after leaving. Do not assume one sample proves every edge case.
3. Console: `gamemode creative @a`. Rapidly press/release/press Space, hold second press, then release. Verify actual server `data get entity @p abilities` shows flying enabled. Hold Space to rise and Shift to descend. Release all input. Repeat toggle off/on.
4. While flying, console `execute as @a at @s run tp @s ~ ~3 ~`; corrected position must resume correctly. Then `gamemode survival @a`; fall/land, verify server abilities flying/mayfly false. No phantom hover.
5. Console: `gamemode spectator @a`. Hold Space to ascend at least two blocks above the pad; release, then `gamemode creative @a`. Check airborne flight is retained. Descend/land: flying turns off while mayfly stays granted. At ground, Spectator→Creative clearing flight is EXPECTED vanilla26.3 behavior, not a client failure. Return Survival afterward.
6. Open/close inventory while moving; no stuck held movement. Disconnect/reconnect and repeat a normal jump plus creative double-tap. Quit normally.

Collect after disconnect (the join report finalizes on session end):

```sh
python3 packaging/macos/collect-java-diagnostics.py "$TEST_DIR"
git rev-parse HEAD > "$TEST_DIR/source-commit.txt"
open "$TEST_DIR"
```

Keep the console folder and `java-join-latest.json`/telemetry; do not upload account files. Server-authored setup teleports cause expected corrections; report unexplained repeated corrections separately.

For another run without changing source, use `make play-run` (or the existing assembled app executable). This skips compilation/assets. After changing branches, build successfully once before using the no-build path.

## 4. Switch to C and build once

Quit A first; leave the disposable server running. Same repo Terminal and same cache/profile:

```sh
git switch --detach origin/candidate/java-lane-c
git rev-parse HEAD
TEST_DIR=$(mktemp -d "$HOME/Desktop/cinnabar-lane-c.XXXXXX")
touch "$TEST_DIR/started"
make play 2>&1 | tee "$TEST_DIR/console.log"
```

1. Join loopback; select wool/shears/wool and move/look. Console setup: `gamemode survival @a`, `tp @a 104.5 69 102.5 0 20.5`, `clear @a`, `give @a minecraft:white_wool 16`, `give @a minecraft:shears`, `setblock 104 69 105 minecraft:stone`, `setblock 104 69 104 minecraft:air`.
2. Place against stone, then hold attack with shears to break the placed wool. Repeat five times. Console `execute if block 104 69 104 minecraft:air` after mining; `data get entity @p Inventory` verifies actual stacks. A later real item pickup may increase the wool count.
3. Start mining then release, look away, change slot and open inventory in separate attempts. No late phantom break or stuck animation. Move the sword/shears in inventory; close/reopen and verify server inventory.
4. Console: `gamemode adventure @a`; `clear @a`; `give @a minecraft:shears[minecraft:can_break={blocks:"minecraft:white_wool"}]`; `setblock 104 69 104 minecraft:white_wool`. Mine wool normally, attempt stone. Wool breaks; stone must remain.
5. Console: `item replace entity @p weapon.mainhand with minecraft:white_wool[minecraft:can_place_on={blocks:"minecraft:stone"}] 3`. Place against stone, then try placing against that wool. Allowed placement consumes once; denied surface retains server stack. Check console Inventory and block state.
6. Console: `gamemode survival @a`; then `transfer 127.0.0.1 25586 @a`. Verify destination terrain/hotbar/counts/HUD and movement. Disconnect/reconnect. Cold held geometry on immediate reconnect and broader equip completeness remain open handoff residuals—record them separately.
7. Disconnect and quit normally, then collect the same way as A:

```sh
python3 packaging/macos/collect-java-diagnostics.py "$TEST_DIR"
git rev-parse HEAD > "$TEST_DIR/source-commit.txt"
open "$TEST_DIR"
```

Also retain the disposable server console output (or its `logs/latest.log`) showing the requested data/block checks. Return both test folders and a short recording of any jitter, ghost block, incorrect stack, or missed gesture. Expected denial reverts and deliberate server teleports are not automatic failures. Do not approve based solely on a swing or local block appearance: compare the server state. Stop the disposable server with its `stop` command afterward. No candidate is merged until your approval.
