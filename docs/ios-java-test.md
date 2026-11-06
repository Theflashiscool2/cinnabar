# Build and test native Java on a Mac/iPhone

Use the revision in the supplied recovery handoff, on `feature/native-java-foundation`
in `Theflashiscool2/cinnabar`. Preserve local changes: if a fast-forward update fails,
stop and inspect `git status`; do not reset or discard them. The published candidate
uses the build number in `packaging/ios/runtime.json`. There is no downloadable IPA
until a Mac/CI build actually produces one.

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
