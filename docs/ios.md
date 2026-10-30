# iOS (experimental)

The iOS client is the desktop client built for arm64 iPhones and iPads on iOS 17+, rendering
with Metal. iOS lets an app neither start other programs nor compile code at run time, so the Go
core links into the app itself, and there is no prep kit: the IPA cannot prepare game assets on
the device. You sideload it yourself; it is not on the App Store. It has run on an iPhone 18 Pro
Max: menus, sign-in, joining servers and shader packs work; touch input is lightly tested.

## Get the IPA

The iOS workflow builds `Cinnabar-<version>-build<build-number>-unsigned.ipa` and attaches it to the run as
`cinnabar-ios-unsigned`. To get one, push a branch named `ios/<anything>` to your fork of the
repository (enable Actions on the fork first) or run the iOS workflow by hand, then download it
with `gh run download <run id> -R <you>/cinnabar -n cinnabar-ios-unsigned`. On a Mac with
Xcode 27.1, the iOS Rust target (`rustup target add aarch64-apple-ios`), Go and librsvg or
ImageMagick, build it locally instead:

```sh
bash packaging/ios/build-ipa.sh
```

The IPA lands in `target/ios/`. Pinned values (bundle id, deployment target, Xcode major) live in
`packaging/ios/runtime.json`.

## Scene lifecycle and launch qualification

SDK 27 builds require scene adoption. The old build trapped at
`UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption` on a physical
iPhone running iOS 27.0 beta. Build 31.1 declares one `UIWindowScene` and uses the
immutable Winit 0.30.13 backport pinned in `Cargo.toml` and `Cargo.lock`.
This is a **candidate repair, not a device-qualified build**.

[Winit PR 4723](https://github.com/rust-windowing/winit/pull/4723) explains the
blank-window failure when only a scene manifest is added. Its
[0.30 backport](https://github.com/molenick/winit/commit/94d6a23c425023810a0879aa7a03abb2e4f1decc)
attaches the window to the connected `UIWindowScene`. The PR is still unmerged;
the latest published release inspected was 0.30.13. The git dependency also uses
its own `dpi` 0.1.1 package. No installed Cargo-cache files are modified.

UIKit creates the single scene from the manifest. No custom scene delegate is
required by this notification-based Winit integration. The existing Bevy runner
creates the window on `resumed`; Winit attaches its existing root controller to
the connected scene before exposing the window to the renderer. Its application
notification observers continue to route activation, suspension, occlusion, memory
warnings and termination. The existing UIKit view handles layout/rotation, scale,
safe-area queries and touch input. There is one engine and one event loop, with no
storyboard-created second window. No URL callback was registered in this host;
device-code authentication remains unchanged.

References: Apple's [scene configuration](https://developer.apple.com/documentation/uikit/specifying-the-scenes-your-app-supports)
and [scene lifecycle migration](https://developer.apple.com/documentation/uikit/migrating-to-the-uikit-scene-based-life-cycle).
Direct Apple-document retrieval was blocked in the Linux environment; the upstream
patch and pinned UIKit bindings were inspected. Upstream simulator results are
reference evidence, not Cinnabar launch results.

After rebuilding with the existing workflow, reuse the already compiled assets:

```sh
CINNABAR_IOS_PROFILE=play bash packaging/ios/build-ipa.sh
python3 packaging/ios/add-assets.py target/ios/Cinnabar-0.1.31-build31.9-unsigned.ipa
python3 packaging/ios/verify-ipa.py target/ios/Cinnabar-0.1.31-build31.9-assets.ipa --require-assets
```

The verifier inspects the archived plist, device Mach-O header, single-scene
configuration, build number, source commit and bundled startup resources, and
prints the archive's SHA-256. It does not prove linkage, valid signing, carrier
integrity or successful execution. Sideloadly must sign the new assets IPA; the
build script's ad-hoc signature is not an installation identity. The old IPA is
preserved under its old filename.

Acceptance still requires a real menu frame, navigation/touch, background and
foreground, both supported landscape orientations/safe areas, terminate/relaunch,
Bedrock and Java menu selection, and a controlled world smoke test. Also exercise
scene disconnection/reconnection: this backport adds attachment at window creation,
not a new disconnected-scene recovery implementation. Do not infer that recovery
from compilation. Record the packaged build number, source commit, OS and device alongside screenshots
and logs; any new crash needs its `.ips` report. Linux cross-checks of the UIKit
crate are neither simulator execution nor an iPhone build.

## Add your assets

The IPA carries no Mojang-derived files. Compile the carriers on your own computer, then copy
them and the vanilla resource pack into the IPA:

```sh
make assets
python packaging/ios/add-assets.py path/to/Cinnabar-<version>-unsigned.ipa
```

This writes `Cinnabar-<version>-assets.ipa` beside the input (`--out` picks another path,
`--local` another `.local` directory). The assets stay on your machine and device.

## Sideload from Windows

Either tool signs the IPA with your Apple ID. A free Apple ID signs it for 7 days; sideload it
again to renew. Either way, after the first install enable **Settings > Privacy & Security >
Developer Mode**, restart when asked, and if iOS says the developer is untrusted, trust it in
**Settings > General > VPN & Device Management**.

- [iloader](https://iloader.app/) works with iTunes from the Microsoft Store: connect the device
  by USB, trust the computer, sign in, and choose **Import IPA** with the `-assets` IPA.
- [Sideloadly](https://sideloadly.io/) needs iTunes and iCloud from Apple's website instead of
  their Microsoft Store versions; open the `-assets` IPA in it and sign in.

iOS stops an app that passes its memory limit, about 3.3 GB on a 12 GB iPhone, and servers that
send large resource packs can pass it while the world loads. The App ID's **Increased Memory
Limit** capability raises the limit. iloader's Import IPA leaves it off, but once it is on for
the App ID, later installs keep it; `isideload`'s `install_app` turns it on when its
`increased_memory_limit` argument is true.

## Logs and configuration

The app's `Documents` folder appears in the Files app under **On My iPhone > Cinnabar**. Logs are
in `data/logs`, settings and `servers.json` in `config`. Include the newest log when reporting a
problem. The build includes the experimental Java shader-pack renderer: drop a pack's zip into
`data/shaderpacks` and pick it in the Shaders tab. Packs render at the screen's native 3×
resolution, so a heavy pack is GPU-bound on a phone; its lower quality profiles cost less.

## Controls

The same on-screen touch controls as on Android: the stick moves, dragging elsewhere turns,
and the buttons jump, sneak, sprint, attack, use, pause and open the inventory. Menu text fields
and chat ask iOS for its on-screen keyboard; nothing yet moves a field the keyboard covers. A Bluetooth
keyboard and mouse work only as far as winit's UIKit backend delivers their input.

## Not supported

- Local worlds, mods and anything that needs JIT or a second process.
- Preparing assets on the device: inject them with `add-assets.py` before sideloading.
- The crash monitor and native crash dumps; Rust panics still write crash folders.
- Clipboard. Browser links use UIKit on iOS; if the handoff fails, use the displayed
  device-code URL and code in a browser manually.

## Native Java Edition in this fork

The iPhone runtime observations above describe upstream's Bedrock path. They do not
qualify native Java networking, authentication, terrain, touch gameplay or Hypixel.
The Java release target remains the version in `crates/java-protocol/data/target.json`.

Java terrain and entity workers resolve resources through the host's `InstallLayout`,
including when UIKit changes the working directory. To include locally prepared Java
inputs alongside the required shared carriers:

```sh
python3 packaging/ios/add-assets.py path/to/Cinnabar-<version>-unsigned.ipa \
  --java-assets path/to/prepared-java-runtime
```

The directory must contain `client.jar`, `blocks.json`, `physics.bin`,
`interaction.bin` and `models.json`. The injector verifies the bytes it writes
against the tracked Java target's sizes and SHA-256 hashes. A mismatch leaves any
existing output untouched. It copies only these Java files into
`local/assets/java/<pinned-version>/`; server binaries, exporter programs and account
credentials are excluded. Prepare exports on a host with the target's Java runtime,
using the existing `assetc java-physics`, `java-interaction` and `java-entities`
commands and their explicit EULA acceptance. The running iPhone app never executes
Java or a data exporter. Desktop `CINNABAR_JAVA_ASSETS` overrides still work.

The build script targets Xcode 27.1 and iPhoneOS SDK 27.1, as recorded in
`packaging/ios/runtime.json` (patch updates within that version are allowed).
The minimum device OS remains iOS 17. The script respects an explicit
`DEVELOPER_DIR`, otherwise uses the selected matching Xcode or a matching
side-by-side install. Check the tools before starting the build:

```sh
bash packaging/ios/build-ipa.sh --check-toolchain
rustup target add aarch64-apple-ios
CINNABAR_IOS_PROFILE=play bash packaging/ios/build-ipa.sh
```

This pin is an intended build target, not proof of an iOS build or device test.
The CI runner OS label is independent of the Xcode version; its preflight fails
if that runner image does not contain the pinned Xcode. Select an available
matching installation rather than bypassing the SDK check.

An unsigned IPA still needs the user's asset injection, signing and device
installation. Java Accounts is reached through Accounts → Java Accounts, or by
joining a Java server before selecting a Java account. Add/sign in again opens
Microsoft device authorization; choose the intended Microsoft account there.
The picker supports selection, refresh and removal independently of Bedrock.
Closing cancels authorization and the pending join; reopening preserves saved
accounts. During sign-in the chosen server remains visible on the Join button
once the selected account is ready. Account management is disabled while a world
is connected or connecting; switching never replaces an active session identity.

When no application is configured, Java Accounts opens **Java sign-in setup**.
Its registration button opens Microsoft Entra in the browser; the owner completes
registration there and returns to paste the public **Application (client) ID**.
**Save ID and sign in** validates and saves the ID on the device, then starts the
existing device-code flow. No JSON editing or rebuild is needed. The public ID
survives restart; **Application setup** lets the owner change it later. Existing
accounts must reauthorize if their application identity changes.

Configure an owned Microsoft public-client application with personal-account
support and public-client device authorization enabled. No client secret or
redirect URL is required for this device-code flow. Application registration
does not establish Minecraft service-access approval; the owner must obtain
that separately where required; the app cannot register or approve itself without
the owner's Microsoft authorization. The in-app setup stores only its public ID in
`Documents/data/auth/java/oauth-client.json` in this app's Files container:

```json
{"client_id":"YOUR-OWNED-PUBLIC-APPLICATION-ID"}
```

The official iOS build embeds the owner's public ID
`76e54634-fa88-4777-ae76-41d004eecebc`, so ordinary users go directly to
**Sign in with Microsoft**. This identifier is public and forks can replace it
through the setup screen; no secret is embedded.

iOS reusable Java credentials use Keychain generic-password items under service
`app.cinnabar.java.accounts`, keyed by verified Java profile UUID. Items are
accessible only while unlocked and do not migrate to another device through a
backup. Metadata/selection stay in the Java account directory; Keychain failures
do not fall back to plaintext. Desktop's existing private credential-file backend
is unchanged. Old plaintext iOS credentials are not imported into the new picker;
reauthorize instead. No account tokens should enter a source bundle or IPA.

Minecraft login rejections now show the HTTP status, an allowlisted symbolic error
code and a UUID request identifier when supplied. Arbitrary response messages and
tokens are never displayed. A 403 alone does not prove an approval restriction.
The exchange was compared with `minecraft-launcher-lib`'s `microsoft_account.py`
at commit `2eec8cd52dcf0f9c5e1faa92edd642252d92de80`: scope, RPS ticket, relying
parties and identity-token format agree. Cinnabar uses the user hash in the XSTS
claims with the XSTS token. No reference implementation code was copied.

The integrated flow still requires physical-device Keychain/browser acceptance,
legitimate authorization, and controlled online-server testing. Compilation and
fixture accounts are not evidence of successful Microsoft or Hypixel access.
