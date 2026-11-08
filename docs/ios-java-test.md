# Build and test native Java on a Mac/iPhone

Use the revision in the supplied recovery handoff, on `feature/native-java-foundation`
in `Theflashiscool2/cinnabar`. Preserve local changes: if a fast-forward update fails,
stop and inspect `git status`; do not reset or discard them. The published candidate
uses the build number in `packaging/ios/runtime.json`. There is no downloadable IPA
until a Mac/CI build actually produces one.

## Fast Mac test cycle and diagnostics

Keep the same checkout path, Rust toolchain, `play` profile and existing APFS Cargo/Go
cache paths between runs. Do not run `cargo clean`, switch profiles, or change
`RUSTFLAGS` while iterating. The `play` profile already disables LTO and enables
incremental compilation; `CARGO_INCREMENTAL=0` overrides that benefit. Enabling it
can rebuild workspace crates once; changed dependencies still need compilation.
No command guarantees a short cold build, especially on unreliable external storage.

From the existing checkout, after mounting the existing APFS build volume:

```sh
git status --short --branch
git pull --ff-only origin feature/native-java-foundation
export CARGO_BUILD_JOBS=2
export CARGO_INCREMENTAL=1
export CINNABAR_JAVA_ASSETS="$PWD/.local/assets/java/26.3"
export CINNABAR_SLOW_FRAME_LOG=0
export CINNABAR_LOG='warn,java_session=debug,java_auth=info,client_shell::menu=info'
TEST_DIR=$(mktemp -d "$HOME/Desktop/cinnabar-java-test.XXXXXX")
touch "$TEST_DIR/started"
make play 2>&1 | tee "$TEST_DIR/console.log"
```

Keep existing `CARGO_BUILD_BUILD_DIR`, `CARGO_TARGET_DIR`, `GOCACHE` and account/user-root
settings. Check that their volumes are actually mounted before creating directories.
macOS account/game IPC uses a profile-isolated directory under `/tmp/cinnabar-ipc-*`,
so Unix sockets do not depend on the checkout or profile volume supporting them.
This does not move your accounts, logs, assets, or build caches.
Stop on a failed pull or build. A failed command piped to `tee` may look successful;
use `set -o pipefail` in the test shell. After a successful build, **relaunch the same
binary without compilation or asset preparation**:

```sh
make play-run 2>&1 | tee "$TEST_DIR/console-relaunch.log"
```

On ExFAT this assembly can copy large binaries because hard links are unavailable.
For an already assembled, unchanged build, launch it directly to skip those copies:

```sh
"target/play/Cinnabar Dev.app/Contents/MacOS/cinnabar" 2>&1 | tee "$TEST_DIR/console-relaunch.log"
```

After a source update, build successfully once before using either no-build route.
For each attempt, disconnect in the application before collecting the join report;
it is finalized when the connection ends. Quit normally so log writes finish. Then:

```sh
python3 packaging/macos/collect-java-diagnostics.py "$TEST_DIR"
open "$TEST_DIR"
```

The collector copies only `client.log`, `core.log` and `java-join-latest.json` modified
since the test started, checking the existing user-root, local logs and Cinnabar
Application Support directories. It does not copy account/token stores, reuse old
reports as current evidence, or overwrite earlier collections. Share the test folder
privately, plus the exact chat rejection/screenshot. For hits, record the action,
visible response and any damage/health change; for loading, record which world,
players, skins or menu appeared late. For double-tap sprint, press/release/press W
within the native seven-tick window and hold the second press; also test release,
sneaking, menu/focus changes and reconnect. Mac results do not establish iPhone acceptance.

If the app was already open when the test folder was created, recover its existing
logs without rebuilding:

```sh
RECOVERED_LOGS=$(mktemp -d "$HOME/Desktop/cinnabar-existing-logs.XXXXXX")
python3 packaging/macos/collect-java-diagnostics.py "$RECOVERED_LOGS" --recover-existing
open "$RECOVERED_LOGS"
```

Older logs are explicitly labelled in the manifest and may belong to another
build or attempt. Include the build revision and approximate attempt time.
Disconnect first to finalize the join report.

If the app was already running before the test folder was created, recover its
existing logs without building again:

```sh
RECOVERY_LOGS=$(mktemp -d "$HOME/Desktop/cinnabar-existing-logs.XXXXXX")
python3 packaging/macos/collect-java-diagnostics.py "$RECOVERY_LOGS" --recover-existing
open "$RECOVERY_LOGS"
```

This explicitly includes older allowlisted logs. The manifest labels their original
paths, modification times and unverified provenance; it does not identify them as
this run's evidence. A console containing only `assetc prepare` has no gameplay
trace. Disconnect to flush the join report, then collect again if needed.

## Prerequisites and inputs

Use macOS, full Xcode/iPhoneOS SDK **27.1** (patch updates allowed), the pinned Rust
in `rust-toolchain.toml`, `aarch64-apple-ios`, Go, Python 3.11+, zip, and either librsvg
or ImageMagick. Run from the repository root. Existing cache and asset directories
can be reused. Put Cargo intermediate/target caches on a mounted APFS volume with
space for linking; do not format a drive or overwrite its contents for this test.
A Java runtime is not used by the iPhone client. JDK25/server artifacts are needed
only when regenerating the pinned exported data or running the controlled server.

Shared carriers and installed vanilla pack live under `.local/assets/compiled/`
and the path selected by `assets/vanilla-source.json`. Validate/reuse them through
the existing `make assets` pipeline if necessary. The prepared Java directory is
`.local/assets/java/<game_version from crates/java-protocol/data/target.json>/`:
`client.jar`, `blocks.json`, `physics.bin`, `interaction.bin`, `models.json`,
`fluids.json`. Do not add account files, exporters or server.jar to this directory.
The six inputs must match that manifest's sizes and SHA-256 values.

```sh
TOOLCHAIN=$(python3 -c 'import tomllib; print(tomllib.load(open("rust-toolchain.toml", "rb"))["toolchain"]["channel"])')
rustup toolchain install "$TOOLCHAIN"
rustup target add aarch64-apple-ios
bash packaging/ios/build-ipa.sh --check-toolchain
JAVA_VERSION=$(python3 -c 'import json; print(json.load(open("crates/java-protocol/data/target.json"))["game_version"])')
python3 packaging/ios/add-assets.py --check-inputs \
  --local "$PWD/.local" --java-assets "$PWD/.local/assets/java/$JAVA_VERSION"
```

This preflight checks Java hashes and shared-directory presence. It does not claim
shared-carrier integrity or runtime success; the asset compiler and client perform
their own carrier validation. A missing input must be prepared through the existing
`make java-assets JAVA_ACCEPT_EULA=1` pipeline after agreeing to the Minecraft EULA.
Existing prepared inputs need no JDK installation or downloads again.

## Build, package and sign

```sh
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 CINNABAR_IOS_PROFILE=play \
  bash packaging/ios/build-ipa.sh
BUILD=$(python3 -c 'import json; print(json.load(open("packaging/ios/runtime.json"))["build_number"])')
VERSION=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')
IPA="target/ios/Cinnabar-$VERSION-build$BUILD-unsigned.ipa"
python3 packaging/ios/add-assets.py "$IPA" \
  --local "$PWD/.local" --java-assets "$PWD/.local/assets/java/$JAVA_VERSION"
python3 packaging/ios/verify-ipa.py "${IPA%-unsigned.ipa}-assets.ipa" --require-assets --require-java
```

The verifier prints the actual source commit, identifier and SHA-256. Check the
source commit against the handoff; a `-dirty` build is not the exact clean revision.
Do not overwrite a retained assets IPA with `--force` just to suppress an error.
Use the **assets IPA** with the established sideloading tool and your own Apple
signing identity. Ad-hoc signing from the build script is not installation signing.
No signing key, password or credential should be sent in chat. No Linux Vulkan,
Xvfb or lavapipe environment setting belongs in this iOS build/run procedure.

The existing GitHub iOS workflow is an alternative on-demand Mac build. On a machine
with authorized GitHub access, run:

```sh
gh workflow run ios.yml --ref feature/native-java-foundation -R Theflashiscool2/cinnabar
```

Inspect that run's actual `headSha`, and download its
`cinnabar-ios-unsigned` artifact. It contains no game assets; use the same injection
and verification commands on the downloaded IPA. Do not assume a queued or green
workflow proves simulator/device launch. A visible workflow definition does not establish an available runner: inspect the
actual run status and artifact before claiming an Apple build.

## Controlled authenticated acceptance

Use the existing native desktop app first if quicker to iterate (`make play` with
its prepared assets). Clear `CINNABAR_JAVA_OFFLINE_FIXTURE` for authenticated tests;
normal Java joins capture the selected Java profile and never read Bedrock credentials.
Use **Accounts → Java Accounts** to add/select accounts, refresh an expired account,
cancel a grant, and restart to check persistence. iOS stores reusable credentials in
Keychain with device-only unlocked access; desktop uses the private cache described
in [Java authentication acceptance](java-auth-testing.md).

Start the pinned server in a **new disposable directory** with JDK25. Set
`SERVER_JAR` to your existing legitimate download and `JAVA_HOME` to your JDK25
installation (it must contain `bin/java`). From the checkout, after accepting the
[Minecraft EULA](https://www.minecraft.net/eula):

```sh
export SERVER_JAR="$HOME/Downloads/server-26.3.jar"
if python3 - <<'PY'
import hashlib, json, os, pathlib
m = json.load(open('crates/java-protocol/data/target.json'))
p = pathlib.Path(os.environ['SERVER_JAR'])
assert p.stat().st_size == m['server_bytes'], 'server size mismatch'
assert hashlib.file_digest(p.open('rb'), 'sha256').hexdigest() == m['server_sha256'], 'server SHA-256 mismatch'
print('Verified server:', m['game_version'], 'protocol:', m['protocol'])
PY
then
ACCEPTANCE_DIR=$(mktemp -d "${TMPDIR:-/tmp}/cinnabar-online.XXXXXX")
printf 'eula=true\n' > "$ACCEPTANCE_DIR/eula.txt"
printf '%s\n' server-ip=127.0.0.1 server-port=25566 online-mode=true \
  enforce-secure-profile=true enable-rcon=false enable-query=false allow-flight=false \
  > "$ACCEPTANCE_DIR/server.properties"
(cd "$ACCEPTANCE_DIR" && "$JAVA_HOME/bin/java" -Xms256M -Xmx768M -jar "$SERVER_JAR" nogui)
else
  echo 'Server verification failed; nothing launched.' >&2
fi
```

Use an absolute `SERVER_JAR` path. If verification fails, stop rather than launching
it. The official source URL and expected hashes are in the same tracked target
manifest; no mirror is required. This keeps normal secure-profile checks and leaves
the offline fixture's world/properties untouched. Wait for the real server `Done`
log, then join `127.0.0.1:25566` through
the desktop **Java** server entry with the selected legitimate account. Both client
and server need Mojang/Microsoft network access. A ping or offline fixture is not
this acceptance. An iPhone must use an intentionally reachable controlled server;
`127.0.0.1` on the phone does not refer to the Mac. Do not expose this fixture publicly
or turn off online authentication to make a connection pass.

Record account selection (no tokens), successful authenticated join, movement and
landing, correction handling, menus/input release, disconnect/reconnect, and app
restart. On iPhone also inspect touch/look, safe areas, keyboard dismissal and
background/resume. Preserve `client.log`, `java-join-latest.json`, server logs and
actual screenshots privately; iOS logs appear in Files → Cinnabar → data → logs.
Log/report encryption, authentication, profile-key, protocol or unsupported-feature
rejections verbatim after removing tokens. Do not mark a sent packet as an accepted
transaction. Keep controlled online, iOS simulator/device, Hypixel lobby and actual
game-mode results separate. Hypixel status text is advisory; only normal legitimate
joining establishes whether this exact implemented version is accepted. Do not
change its handshake number, automate matchmaking/purchases, or fake chat signatures.

## User-directed iPhone server-transition test

After installing the verified **assets IPA**, check the displayed build identifier
against the verifier output. Keep the existing account; no new application registration
is needed. This route is physical-device acceptance, separate from controlled Linux tests:

1. Select the Java account and join the Hypixel lobby. Check players, held items,
   sidebar and touch movement/look. Do not treat successful discovery as joining.
2. Use the NPC/game menu normally and select the intended game. After the backend
   transition, check that the destination replaces the old world, sidebar and menu;
   touch input must remain usable. Network transfers and same-socket world changes
   are different protocol paths; record which destination/action failed.
3. Make one normal shop purchase. Check the server-confirmed price, item count and
   empty cursor, then equip, place and break where permitted by the game.
4. Exercise normal combat/death/Respawn where available. A saved-dead join must expose
   Respawn rather than wait indefinitely for terrain. Reconnect and restart to check
   the selected account and destination state remain correct.

If any step stalls or fails, take a screenshot and note the action and time. Use the
normal Disconnect action if possible so the automatic join report is flushed; preserve
`client.log` and `java-join-latest.json` from Files → Cinnabar → data → logs before
another attempt replaces the latest report. Never share credential caches, cookies or
tokens. A controlled fixture proves neither the actual Hypixel shop type nor Bed Wars/
Duels completion. Merchant trading remains deferred unless a real trace requires it.
