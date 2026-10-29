# iOS (experimental)

The iOS client is the desktop client built for arm64 iPhones and iPads on iOS 17+, rendering
with Metal. iOS lets an app neither start other programs nor compile code at run time, so the Go
core links into the app itself, and there is no prep kit: the IPA cannot prepare game assets on
the device. You sideload it yourself; it is not on the App Store. It has run on an iPhone 18 Pro
Max: menus, sign-in, joining servers and shader packs work; touch input is lightly tested.

## Get the IPA

The iOS workflow builds `Cinnabar-<version>-unsigned.ipa` and attaches it to the run as
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
- Clipboard and opening links. Sign in with the device code on another device.

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
installation. This fork's Java sign-in UI and online/iPhone acceptance remain
incomplete; no account tokens should be transferred in a source bundle or IPA.
