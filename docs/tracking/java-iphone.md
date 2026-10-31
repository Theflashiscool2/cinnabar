# Native Java iPhone acceptance

The recoverable branch includes native Java networking and earlier Linux fixtures.
Missing unpublished work from the previous execution environment is not assumed to
exist. The official iOS host/Metal renderer/touch controls and packaging are reused.

| Milestone | Implementation and next action | Verified evidence | Open acceptance / blocker |
| --- | --- | --- | --- |
| iOS host and shared resources | Single-scene manifest and pinned Winit scene-attachment backport; build 31.1 | Owner reports successful patched build, iPhone menu access and existing Bedrock sign-in/join paths; no agent-inspected device captures | Orientation, background/resume and disconnected-scene recovery remain unverified; no connected Apple runner/device |
| iOS browser/account diagnostics | Build 31.2 candidate routes web links through UIKit; Java account prerequisite precedes server discovery; explicit-port DNS uses the OS resolver | Browser module cross-checked for iOS; device handoff remains pending | Native Java account UI and legitimate application configuration still required; Bedrock sign-in does not qualify Java |
| Java runtime assets | Shared layout discovery for terrain, extensions and entities; hash-checked `add-assets.py --java-assets` | Linux Rust layout regression and synthetic IPA injection tests | Real pinned inputs, iOS build and device loading not verified |
| Microsoft account | Native account UI, device-flow/refresh, iOS Keychain and desktop private cache; approved fork application ID configured | Owner reports successful Mac authentication probe and iPhone sign-in; supplied Mac join log records Minecraft session authentication HTTP 204 | Refresh/account-switch acceptance remains separate; no further registration request is currently indicated |
| Controlled authenticated Java server | Native session path retained separately from Bedrock | Linux compilation; historical offline fixtures are not online qualification | Controlled online-mode server acceptance still pending |
| Hypixel lobby | Preserve pinned Java 26.3/protocol 777 implementation and show specific connection failures | Owner's Mac join authenticates, then reaches the client's unsupported play-tag gate; no successful lobby entry established | Tag update implementation now needs the owner to retry joining; further protocol/gameplay gaps may follow |
| Registry tag updates | Decode configuration/play updates, replace supplied registries atomically, retain omitted registries, clear empty sets; ordered session admission and bounded retained memory | Compressed TCP fixture covers updates before/after world login, malformed packets and continued keepalives; admission fixtures cover replacement and retired sessions | Real updated-build join pending; this does not establish support for every tag-driven gameplay predicate or mid-session reconfiguration |
| iPhone controls and server menus | Trace shared touch/keyboard paths into native actions and transactions | Not iOS-tested | Actual touch movement/look, hotbar, attack/use, inventory and text-input acceptance |
| Duels playability | Provisional first mode; finish required movement, inventory, entities/combat and reconnect | Not attempted | Lobby acceptance, official mode/version requirements and user-directed public play |

The release is not complete. Compilation and synthetic packaging tests do not
establish iOS, physical-device, online-server or Hypixel acceptance. Asset preparation
and Mac commands are in [the iOS guide](../ios.md#native-java-edition-in-this-fork).
