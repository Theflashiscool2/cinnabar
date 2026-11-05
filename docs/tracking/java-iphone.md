# Native Java iPhone acceptance

The recoverable branch includes native Java networking and earlier Linux fixtures.
Missing unpublished work from the previous execution environment is not assumed to
exist. The official iOS host/Metal renderer/touch controls and packaging are reused.

## Required playable flow (current evidence, 2026-11-05)

| Step | Current implementation / evidence | Remaining implementation or acceptance | External dependency |
| --- | --- | --- | --- |
| 1 Open app | Owner verified physical-iPhone menu; scene lifecycle retained | New changes not iOS tested | Mac/Xcode and device execution |
| 2 Select separate Java account | Native manager; joins capture selected profile and reject a later selection change/logout; developer offline fixtures cannot override account-backed joins | Device switching/cancellation/restart acceptance | Owner account/device |
| 3 Authenticate/persist securely | Existing approved public-client flow and iOS Keychain; owner login passed | Refresh/expiry/multi-account device acceptance | Legitimate interactive account |
| 4 Controlled online server | Native encryption/session auth retained | Agent online-live acceptance pending | Account and authorized online infrastructure |
| 5 Hypixel lobby | Owner Mac lobby reached; protocol777 preserved | Repeated backend handoff and iPhone lobby qualification | Legitimate account/user-directed test |
| 6 World/players/items/server UI | Native rendering/roster/inventory/sidebar; Linux offline evidence | Native ordinary/special inventory icons now inspected in Linux vanilla; held/equipment presentation, dynamic item context and full server UI remain incomplete | Independent implementation available |
| 7 Touch gameplay/menus/chat | Existing touch and semantic actions retained | Full touch action/menu/text acceptance, chat integration | Device for acceptance; implementation independent |
| 8 Play complete Bed Wars | Some movement, inventory, block interaction and melee exist | Mining/use/combat/death/shop/session loop incomplete | Public matchmaking stays user-directed |
| 9 Reconnect/restart | Controlled offline reconnect evidence; secure persistence implemented | Authenticated/iPhone restart and backend cleanup acceptance | Account/device for acceptance |

This is the finite primary target. No row is qualified by ping, a decoder, upstream
footage or a unit test alone. Terrain/weather cosmetics are deferred unless blocking it.

## Implementation queue (independent of external acceptance)

These are required code gaps, not hardware/account blockers. Work follows existing
owners; no second network, inventory, simulation or world authority is introduced.

| Required capability | Implementation owner / remaining work | Completion evidence required |
| --- | --- | --- |
| Item and held artwork | `assets/registry/java/items`, `app/session/java_render`, shared `SessionIcons`: native GUI model/tint/special factories integrated, with authoritative component patches and scoped workers. **Remaining:** first-person/remote equipment, owner/world predicates, animation and full pack layering | Inspected Linux vanilla icons, component updates, chest transfers, unload/revisit and reconnect; held/reload/iOS acceptance still pending |
| Inventory/menu actions | `inventory/java`, `client-ui/ui_runtime/java_inventory`, `app/session/java_inventory`: finish outside/cursor drop, offhand swaps and required specialized menus; preserve state/incarnation fencing | Controlled accepted/rejected transfers, rapid menu replacement and authoritative counts |
| Entities and combat/lifecycle | `client-world/java`, native entity presentation, `gameplay/java`: equipment/skins/poses, representative entities, ranged/shield/use, damage/death/respawn | Controlled counterpart and server-confirmed results, ID reuse and reconnect |
| Block/item actions | `gameplay/java`, native outbound: full survival mining/progress/cancel, placement/offhand and use precedence, multi-stage use | Actual inputs, denied-action recovery and server state |
| Server UI/chat | `java-protocol/session`, `client-world/java/scoreboard`, shared chat/HUD: chat/command/ack signing, suggestions and remaining rich text/HUD surfaces | Controlled formatted menus, teams, tab, titles/boss bars, chat/commands |
| Packs and transitions | Native session/configuration and resource ownership: transfer consent/destination, full Java pack offer/application/status, stale work retirement | Controlled reconfiguration/backend changes, reload and world/session cleanup |
| iOS actions | Existing semantic/touch/menu/text paths: connect every implemented Java action, text input, safe-area and background release | Shared input fixtures now; actual device flow remains external acceptance |
| Bed Wars then Duels | Integrate the above through lobby, shop/build/combat/death and return; track modes separately | Controlled implementation fixtures plus user-directed real mode acceptance |

## External acceptance gates

- **Protocol/mode qualification:** implementation remains Java26.3/protocol777.
  Prior owner lobby access is retained; current official mode requirements and actual
  lobby/Bed Wars/Duels compatibility are not established by status text. This executor's
  enforced policy excludes Hypixel support/game hosts. Do not change the handshake integer.
- **Authenticated online:** approved application retained; needs permitted auth/session
  hosts and one legitimate interactive account login. Offline fixtures are separate.
- **Apple:** no accessible Apple runner established; Mac/Xcode build, simulator/device
  launch, Keychain, touch and background/resume acceptance remain pending. Packaging
  fixtures do not establish an IPA or physical-iPhone acceptance.

| Milestone | Implementation and next action | Verified evidence | Open acceptance / blocker |
| --- | --- | --- | --- |
| iOS host and shared resources | Single-scene manifest and pinned Winit scene-attachment backport; build 31.1 | Owner reports successful patched build, iPhone menu access and existing Bedrock sign-in/join paths; no agent-inspected device captures | Orientation, background/resume and disconnected-scene recovery remain unverified; no connected Apple runner/device |
| iOS browser/account diagnostics | Build 31.2 candidate routes web links through UIKit; Java account prerequisite precedes server discovery; explicit-port DNS uses the OS resolver | Browser module cross-checked for iOS; device handoff remains pending | Native Java account UI and legitimate application configuration still required; Bedrock sign-in does not qualify Java |
| Java runtime assets | Shared layout discovery; six pinned inputs; packaging preflight and `verify-ipa.py --require-java` check sizes/hashes inside the actual bundle | Prepared six-file preflight and 14 synthetic packaging tests pass; owner generated reports on Mac | [Mac/iPhone handoff](../ios-java-test.md), candidate build31.11; no new IPA produced here; in-app provisioning remains missing |
| Microsoft account | Native multi-account UI, generation-fenced cancellation, captured profile join routing, expiry refresh, iOS device-only unlocked Keychain with no file fallback; desktop private cache; approved application ID retained | Eleven actual-source account tests; inspected Linux/lavapipe separate Java list, two missing-credential selections with actionable message, cancel/return; fixtures contain public metadata only. Owner previously passed Mac/iPhone login | New changes not iOS/online tested. Current environment lacks account credentials, auth-host egress and accessible Apple runner; no further registration request indicated |
| Controlled authenticated Java server | Native session path retained separately from Bedrock | Linux compilation; historical offline fixtures are not online qualification | Controlled online-mode server acceptance still pending |
| Hypixel lobby | Pinned Java26.3/protocol777 retained; native backend configuration handoff repaired | Owner reported authenticated Mac lobby access; new Linux official vanilla debugconfig/reconnect acceptance is separate | Updated-build physical-iPhone/Hypixel handoff still pending; no Bed Wars completion or additional protocol support claimed |
| Item attribute modifiers | Decode and retain target-version modifier amounts, operations, equipment slots and display text through existing inventory authority | Wire vectors, compressed/encrypted session fixture and native inventory admission tests | Live join retry pending; full attribute tooltips and gameplay effects are separate acceptance items |
| Item-component batch and packet census | Every pinned component type has a reader; preserve profiles, books, model data, equipment/use data and nested items in existing inventory patches | Complete registry corpus with following-field alignment, truncation, branching/depth/memory bounds; ordered encrypted inventory delivery and continued keepalives; empty-body dispatch census | No agent live join or iPhone test; metadata decoding does not implement all item behavior. [Known packet gates](../references/java-item-components.md#remaining-packet-gates) remain explicit |
| Logging | Bounded join JSON groups observed packet gaps. Model compilation batches independent state failures into `java-models-latest.json`; `make java-audit` checks the full pinned model registry offline. Slow-frame output stays opt-in | Grouping, sample limits, cancellation and failed-publication tests; reporter/privacy tests retained | First compiler failure per state only; later network traffic and rendering remain outside audit coverage |
| Java fluid presentation | Native state/host facts, liquid geometry, waterlogged hosts, flow/still/overlay materials and Java UVs now reach the shared GPU streams | Verified-data CPU mesh fixtures; real vanilla 26.3 offline world through Linux Xvfb/lavapipe: sources/flow/fall, removal, wet/dry slab, boundary remesh, revisit and reconnect | [Fluid limits](../references/java-fluids.md); nonuniform biomes and special block entities remain separate blockers |
| Java model/tint audit | Native admission covers35,723 states; moving-host production integration supersedes the older12-failure entry | Earlier actual-archive and controlled native special-block evidence; see [current piston scope](../references/java-special-blocks.md#moving-pistons) | Full terrain/resource-pack parity remains unqualified; this item work does not rerun the block-state audit |
| Moving pistons | Ordered event reconstruction, native snapshots/motion, posed GPU publication and static-mesh handoff integrated | Existing native fixtures and inspected Linux vanilla six-direction, sticky, boundary and lifecycle evidence in the reference above | Entity pushing/carrying, custom borders, arbitrary packs and iOS/online remain unqualified |
| Registry tag updates | Decode configuration/play updates, replace supplied registries atomically, retain omitted registries, clear empty sets; ordered session admission and bounded retained memory | Compressed TCP fixture covers updates before/after world login, malformed packets and continued keepalives; admission fixtures cover replacement and retired sessions | Real updated-build join pending; this does not establish support for every tag-driven gameplay predicate or mid-session reconfiguration |
| Java sidebar | Native objective/score/display/reset dispatch into session-owned state and the existing JSON-UI HUD; team decoration and number formats | Official 26.3 codec/state/UI tests; inspected Linux software-rendered vanilla sidebar updates, team slots, fifteen-row limit, removal and reconnect pass | Mac, iPhone and Hypixel sidebar acceptance pending; advanced text and non-sidebar score surfaces remain incomplete |
| Native Java inventory | Native authority now feeds shared JSON-UI HUD/player inventory and all1..6-row generic chest/menu layouts; bounded scope-stamped pickup/split/shift/number-key/collect/drag and hovered-drop clicks, ordinary offhand transfers, default English names and native name components, no local item prediction | Inspected Linux Xvfb/lavapipe vanilla26.3 offline pickup/place/split/merge, chest/double transfers, custom-name/glint items, full-slot refusal, kick/menu cleanup, reconnect and unload/revisit; focused domain/wire/admission/UI/shared-screen regressions | Mac/iOS/Hypixel acceptance pending. Clone/cursor-drop, specialized station layouts, owner/world-dependent item models, animated textures, rich tooltips and remote equipment preview remain incomplete; native idle main/offhand items are integrated. Native GUI artwork now has inspected ordinary/special-item, potion/dye/model override, durability, real chest transfer and reconnect evidence. See [feature inventory](java-server-feature-port.md) |
| Native Java players | Standing/crouching unarmed unit-scale native models, inherited skin layers and reset-safe bone offsets | Two actual native clients: inspected Linux/lavapipe standing↔crouch, motion, reconnect; vanilla admin-fixture confirms pose/bounds. Twelve wide/slim/gait CPU vectors and shared skeletal regressions pass | One default skin inspected live; custom skins/equipment, other poses, Mac/iOS/online/Hypixel remain pending. See [feature inventory](java-server-feature-port.md) |
| Native Java local fluid movement | Native upright water/lava and sprint-swimming, low-pose collision/camera, look steering, clearance recovery and same-world correction continuity | Inspected Linux Xvfb/lavapipe official vanilla26.3 offline tunnel entry, crouch/sprint, drain/crawl, ceiling edits, negative boundaries, corrections and held-input reconnect;46 simulation and4 controller tests | Not iOS/online/Hypixel-tested. Native ladder/trapdoor/scaffolding travel additionally has inspected Linux vanilla offline evidence (all ladder directions, support edits, boundaries/corrections/reconnect);53 simulation tests pass. Remote swimmer/climber animation, bubble callbacks, flight/mounts, broader effects/collision and underwater presentation still incomplete |
| iPhone controls and server menus | Trace shared touch/keyboard paths into native actions and transactions | Not iOS-tested | Actual touch movement/look, hotbar, attack/use, inventory and text-input acceptance |
| Duels playability | Provisional first mode; finish required movement, inventory, entities/combat and reconnect | Not attempted | Lobby acceptance, official mode/version requirements and user-directed public play |

The release is not complete. Compilation and synthetic packaging tests do not
establish iOS, physical-device, online-server or Hypixel acceptance. Asset preparation
and Mac commands are in [the iOS guide](../ios.md#native-java-edition-in-this-fork).

Player-head public profile/texture downloads have bounded parsing and cancellation fixtures but remain live-unvalidated under the current network policy. Local resource patches and native defaults were inspected in offline vanilla gameplay; these do not establish iPhone or Hypixel qualification.

Native bubble-column travel additionally passes inspected Linux vanilla offline upward/downward,
magma-floor, roof/source/neighbor edits, negative boundary, correction, reconnect and revisit
checks. Scaffold ceiling context additionally passes native32-state and inspected fresh-process
offline acceptance; other dynamic ceiling/collision shapes and bubble ambience remain incomplete. This is not iOS or
Hypixel acceptance; no new IPA was produced.

Native first-person idle item presentation now uses target first-person model transforms, real
material arrays, native lighting, alpha modes and the existing shared hand GPU pass. Linux
Xvfb/lavapipe offline vanilla26.3 captures inspected sword/chest/trident and independent
offhand shulker; selection, unload/revisit and new-session reconnect passed. Worker ownership
and obsolete-result tests pass. First capture preceded pipeline readiness; no frame/startup
budget qualification. Swing/equip/active-use animation, owner/world selection, offhand-only
bare-arm composition, remote equipment and iOS/online/Hypixel acceptance remain incomplete.
