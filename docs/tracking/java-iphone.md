# Native Java iPhone acceptance

The recoverable branch includes native Java networking and earlier Linux fixtures.
Missing unpublished work from the previous execution environment is not assumed to
exist. The official iOS host/Metal renderer/touch controls and packaging are reused.

| Milestone | Implementation and next action | Verified evidence | Open acceptance / blocker |
| --- | --- | --- | --- |
| iOS host and shared resources | Single-scene manifest and pinned Winit scene-attachment backport; candidate build 31.1 | Owner built/sideloaded the previous SDK 27.1 IPA; its physical-device startup trapped in UIKit's missing-scene check | Candidate launch, transitions, disconnected-scene recovery and gameplay remain unverified; no connected Apple runner/device in the Linux workspace |
| Java runtime assets | Shared layout discovery for terrain, extensions and entities; hash-checked `add-assets.py --java-assets` | Linux Rust layout regression and synthetic IPA injection tests | Real pinned inputs, iOS build and device loading not verified |
| Microsoft account | Native device-flow/refresh and private cache exist; GUI integration remains necessary in this recovered branch | Existing protocol fixtures only | Owner application registration/service approval and interactive authorization; never use another launcher's identity |
| Controlled authenticated Java server | Native session path retained separately from Bedrock | Linux compilation; historical offline fixtures are not online qualification | Legitimate account plus controlled online-mode acceptance |
| Hypixel lobby | Preserve pinned Java protocol; verify official version requirements before attempting | Not attempted | Version compatibility and authorized account |
| iPhone controls and server menus | Trace shared touch/keyboard paths into native actions and transactions | Not iOS-tested | Actual touch movement/look, hotbar, attack/use, inventory and text-input acceptance |
| Duels playability | Provisional first mode; finish required movement, inventory, entities/combat and reconnect | Not attempted | Lobby acceptance, official mode/version requirements and user-directed public play |

The release is not complete. Compilation and synthetic packaging tests do not
establish iOS, physical-device, online-server or Hypixel acceptance. Asset preparation
and Mac commands are in [the iOS guide](../ios.md#native-java-edition-in-this-fork).
