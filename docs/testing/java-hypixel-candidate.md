# Test the combined Java candidate on Mac

The assistant tested controlled vanilla locally. Your next test is normal manual Hypixel play; no local server or JDK setup is needed. This does not establish physical-iPhone support. Gameplay remains unmerged pending your approval.

Tested candidate: `284101dfd6d8df8eb58f95840566b065eb04fe50`, branch `candidate/java-ac-integration`, Java26.3/protocol777. Native code matches the Linux-tested17ee59d58 executable; the later commit only cleans fixture-area dropped items.

Keep your existing mounted APFS build cache, Rust toolchain, account profile and `play` profile. Do not run `cargo clean` or change RUSTFLAGS. Stop if `/Volumes/CinnabarBuild` is missing; recover the existing image before building. A first build of changed crates may still be slow.

```sh
cd /Volumes/ExternalSSD/CinnabarBuild/cinnabar-iphone-test
ls -ld /Volumes/CinnabarBuild
df -h / /Volumes/CinnabarBuild /Volumes/ExternalSSD
git status --short --branch
```

Preserve any tracked modifications before switching. With a clean tracked tree:

```sh
git fetch origin candidate/java-ac-integration
git switch --detach 284101dfd6d8df8eb58f95840566b065eb04fe50
set -o pipefail
export CARGO_BUILD_JOBS=2
export CARGO_INCREMENTAL=1
export CINNABAR_JAVA_ASSETS="$PWD/.local/assets/java/26.3"
export CINNABAR_JAVA_TELEMETRY=1
export CINNABAR_SLOW_FRAME_LOG=0
export CINNABAR_LOG='warn,java_session=debug,java_auth=info,client_shell::menu=info'
unset CINNABAR_JAVA_OFFLINE_FIXTURE
TEST_DIR=$(mktemp -d "$HOME/Desktop/cinnabar-hypixel.XXXXXX")
touch "$TEST_DIR/started"
make play 2>&1 | tee "$TEST_DIR/console.log"
```

Keep existing CARGO_TARGET_DIR, CARGO_BUILD_BUILD_DIR and GOCACHE paths. To relaunch the same already-built source, use `make play-run` with the same environment. Do not remove the developer-control feature from an existing cache configuration merely for this test.

Select your legitimate Java account showing Ready. Manually test lobby → NPC/menu → backend transition → Bed Wars shop/inventory → movement/place/break → combat/death/respawn → lobby → disconnect/reconnect. Stop and record the exact message if joining or normal actions fail. This is compatibility testing; no automated public matchmaking, purchases or chat.

After disconnecting and quitting, in the same Terminal:

```sh
python3 packaging/macos/collect-java-diagnostics.py "$TEST_DIR"
open "$TEST_DIR"
```

Return the collected diagnostic archive and identify the first failed action. Logs can contain server text or account names; share them privately. Cold held-item geometry on immediate reconnect, broader equip completeness and first-use graphics readiness remain open. Neither local tests nor this Mac test establish iPhone acceptance; no new IPA has been produced.
