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
| 6 World/players/items/server UI | Native rendering/roster/inventory/sidebar; Linux offline evidence | Native ordinary/special inventory icons now inspected in Linux vanilla; idle held items and remote humanoid armor/equipment now inspected offline; dynamic context, custom skins/non-idle poses and full server UI remain incomplete | Independent implementation available |
| 7 Touch gameplay/menus/chat | Existing touch and semantic actions retained | Full touch action/menu/text and authenticated signed-chat acceptance | Device for acceptance; implementation independent |
| 8 Play complete Bed Wars | Some movement, inventory, block interaction and melee exist | Controlled native shop/mine/place/use/two-client combat/death/respawn/world/reconnect loop passed; complete real Bed Wars rules/shop/varied combat and mode acceptance remain incomplete | Public matchmaking stays user-directed |
| 9 Reconnect/restart | Controlled offline reconnect evidence; secure persistence implemented | Authenticated/iPhone restart and backend cleanup acceptance | Account/device for acceptance |

This is the finite primary target. No row is qualified by ping, a decoder, upstream
footage or a unit test alone. Terrain/weather cosmetics are deferred unless blocking it.

## Implementation queue (independent of external acceptance)

These are required code gaps, not hardware/account blockers. Work follows existing
owners; no second network, inventory, simulation or world authority is introduced.

| Required capability | Implementation owner / remaining work | Completion evidence required |
| --- | --- | --- |
| Item and held artwork | `assets/registry/java/items`, `app/session/java_render`, shared `SessionIcons`: native GUI model/tint/special factories integrated, with authoritative component patches and scoped workers. **Remaining:** active equipment poses, non-humanoid/head/trim layers, owner/world predicates, animation and full pack layering | Inspected Linux vanilla icons, component updates, chest transfers, unload/revisit and reconnect; idle held and remote humanoid equipment now inspected offline; full reload/iOS acceptance still pending |
| Inventory/menu actions | `inventory/java`, `client-ui/ui_runtime/java_inventory`, `app/session/java_inventory`: finish outside/cursor drop, offhand swaps and required specialized menus; preserve state/incarnation fencing | Controlled accepted/rejected transfers, rapid menu replacement and authoritative counts |
| Entities and combat/lifecycle | `client-world/java`, native entity presentation, `gameplay/java`: custom skins/non-idle poses, representative mobs/projectiles/NPCs, ranged presentation and active poses; ordinary offhand shielding/use and remote armor/held and basic two-client damage/death/respawn now pass controlled offline evidence | Controlled counterpart and server-confirmed results, ID reuse and reconnect |
| Block/item actions | `gameplay/java`, native outbound: accepted native survival mining/progress/cancel and basic main-hand food/bow use/release; basic both-hand block/entity/air precedence and active-use lifetime now pass controlled vanilla; remaining specialized uses, dynamic same-item component refresh, target-slot fallthrough and Adventure predicates | Actual inputs, denied-action recovery and server state |
| Server UI/chat | `java-protocol/session`, `client-world/java/scoreboard`, shared chat/HUD: unsigned player conversations, disguised announcements, command/system/actionbar routing pass controlled vanilla; native cache/window/checksum framing is integrated. Account-bound certificate preparation and signature/chain verification are connected with synthetic fixtures; authenticated service acceptance remains pending. Server-backed command suggestions now reach the shared editor with native UTF-16 ranges and correlated responses. Native server command trees and ordinary message/alias envelopes now share the existing certificate/index/ACK owner; synthetic keys verify chain continuity. Linux offline whispers, permission/tree replacement, root redirects and reconnect/configuration passed. Complex selectors/UUID/other signed prefixes, string-mode admission hardening, suggestion tooltips/large catalogs, deletion presentation and trust badges and remaining rich text/HUD surfaces remain incomplete | Controlled formatted menus, teams, tab, titles/boss bars, chat/commands |
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
| Native Java inventory | Native authority now feeds shared JSON-UI HUD/player inventory and all1..6-row generic chest/menu layouts; bounded scope-stamped pickup/split/shift/number-key/collect/drag and hovered-drop clicks, ordinary offhand transfers, default English names and native name components, no local item prediction | Inspected Linux Xvfb/lavapipe vanilla26.3 offline pickup/place/split/merge, chest/double transfers, custom-name/glint items, full-slot refusal, kick/menu cleanup, reconnect and unload/revisit; focused domain/wire/admission/UI/shared-screen regressions | Mac/iOS/Hypixel acceptance pending. Clone, specialized station layouts, owner/world-dependent item models, animated textures, rarity/enchantment/effect tooltip details, advanced text and remote equipment preview remain incomplete; native idle main/offhand items are integrated. Native GUI artwork now has inspected ordinary/special-item, potion/dye/model override, durability, real chest transfer and reconnect evidence. See [feature inventory](java-server-feature-port.md) |
| Native Java players | Standing/crouching native models, ordinary humanoid armor, held items and authoritative main/offhand WHACK gestures, inherited skin layers and reset-safe bone offsets | Two actual native clients: inspected Linux/lavapipe standing↔crouch, motion, reconnect; vanilla admin-fixture confirms pose/bounds. Twelve wide/slim/gait CPU vectors and shared skeletal regressions pass | One default skin inspected live; custom skins, active use or non-humanoid equipment, other poses, Mac/iOS/online/Hypixel remain pending. See [feature inventory](java-server-feature-port.md) |
| Native Java local fluid movement | Native upright water/lava and sprint-swimming, low-pose collision/camera, look steering, clearance recovery and same-world correction continuity | Inspected Linux Xvfb/lavapipe official vanilla26.3 offline tunnel entry, crouch/sprint, drain/crawl, ceiling edits, negative boundaries, corrections and held-input reconnect;46 simulation and4 controller tests | Not iOS/online/Hypixel-tested. Native ladder/trapdoor/scaffolding travel additionally has inspected Linux vanilla offline evidence (all ladder directions, support edits, boundaries/corrections/reconnect);53 simulation tests pass. Remote swimmer/climber animation, bubble callbacks, flight/mounts, broader effects/collision and underwater presentation still incomplete |
| iPhone controls and server menus | Trace shared touch/keyboard paths into native actions and transactions | Not iOS-tested | Actual touch movement/look, hotbar, attack/use, inventory and text-input acceptance |
| Bed Wars / Duels playability | Bed Wars first, Duels second; finish required movement, inventory, entities/combat and reconnect | Not attempted | Lobby acceptance, official mode/version requirements and user-directed public play |

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

Native cursor outside-drop now routes normal shared inventory input through scope-stamped
native PICKUP requests with server-owned cursor updates. Inspected Linux software-rendered
offline vanilla26.3 personal/chest tests passed: five→four→empty, blank-inside/no-op safety,
close/reopen and reconnect. Vanilla uniquely named dropped entities confirmed one and four;
the source chest cleared only the tested slot and retained its previous eight-item stack.
Unmapped decorative cursor followers no longer absorb outside clicks; mapped/disabled/modal
controls retain ownership. Six shared screen-policy regressions pass. This establishes cursor
transactions, not dropped-item world rendering, specialized menus, iOS or Hypixel acceptance.

Native menu hover text now preserves literal component names/lore, named colors and inherited
bold/italic styles, native lore defaults and tooltip-display hide flags. Text preparation follows
inventory/name-resource changes. Inspected Linux offline vanilla26.3 captures passed an open
chest price update, hidden-lore/title-only, hidden-tooltip and close cleanup. Six focused source
tests and two public UI regressions pass. This is a controlled chest with shop-style text, not
shop/Hypixel acceptance. Full rarity/enchantment/effect lines, translated arguments/custom fonts,
full pack layering and iOS acceptance remain incomplete.

Native Survival tool mining now has pinned per-state hardness/drop facts, default/patch tool
rules with authoritative block tags, float tick progress, haste/fatigue admission and expiry,
ground/eye-water penalties, ordered START/ABORT/FINISH, and cancellation on identity/input
boundaries. The existing simulation/network service owns held progress during rendering.
Facts are source-bundled; players need no extra export or seventh runtime asset. Controlled
Linux Xvfb/lavapipe vanilla26.3 inspected chest→obtain/equip→mine, abort, haste/fatigue,
accepted/denied placement, x31/32 boundary, correction and same-process reconnect. A live-found
look regression was repaired: position publication no longer overwrites newer look angles
between native ticks. Independent 36 vanilla tool vectors and focused regressions pass.
Native ordinary-model crack stages use the shared GPU pass and target destroy textures;
replacement/removal/reconnect cleanup was inspected. Backpressured targets are revalidated
before START/FINISH, and unsent starts do not emit misleading aborts. Special-model crack
surfaces, swing/audio, offhand precedence and Adventure can-break
predicates remain implementation gaps. This is not Bed Wars, Duels, online or iOS
qualification. Full application suite remains incomplete; the exporter command build exhausted
local disk, while its actual verified Runtime/generator reproduced the pinned report.


## Active item-use integration checkpoint (2026-11-05)

The existing ordered native controller now starts supported main/offhand consumables, bows
and base blocking components. A single bounded click chain follows native main-target/air,
then off-target/air ordering; it preserves the captured hand, rotation and entity hit.
Entity interaction uses the target version’s single hit-bearing packet, without a prediction
sequence. Removed/reused entity identities retire the entire unsent chain, including air
fallthrough. Generated living-method ownership, server food tags and native pig/riding
precedence prevent feeding from incorrectly becoming eating. Exact target defaults/patches, hunger, ammo, cooldowns and
use effects choose native local intent; the server alone changes inventory/effects. A
pinned declaring-method report (1286 blocks/1658 items) permits guaranteed ordinary
UseOn PASS -> UseItem fallback; block/item overrides are not guessed to pass. Its original
`assetc java-use-rules` exporter uses the existing verified-runtime/EULA/output boundaries.
The report is source-bundled, with no extra player-side runtime export.

One client timer follows the world/movement/interaction loan between frames. It advances
cooldowns and effect durations even while collision/rendering waits. Native use slowdown
and sprint restrictions feed the same movement simulation. Early release, main-hand slot changes
and inventory focus queue native RELEASE_USE_ITEM (neutral hit, sequence0) before later
movement/selection. Mining cannot bank time while using an item. Offhand use survives main-hotbar changes and shield wear. Same-authority teleport corrections
retire stale clicks/mining while preserving an admitted item hold; focus/death/session
boundaries still release it. Native living-use flags
and USE_ITEM_COMPLETE=9 are admitted with existing session/epoch protection. Local elapsed
duration never invents server completion, item consumption or a transaction acknowledgement. Controlled Linux vanilla acceptance now covers
offhand food/placement and rejection, chest/secondary-use precedence, actual player-target
shield use and a peer attack (server shield wear with unchanged health), offhand pig feeding
(server breeding state and carrot count), menu/replacement cancellation, mirrored eating,
teleport continuity, dimension changes, reconnect and main-hand food/bow regression.
The native graphical build, gameplay Cargo test compilation, 67 focused source tests and
architecture gate pass. Full application suite remains incomplete. Cold hand-artwork
readiness and active shield/remote poses, full target-equipment fallthrough and dynamic
same-item use-component refresh remain gaps. These results do not qualify iOS or Hypixel.
Native eating/drinking arm transforms use existing held geometry/GPU ownership.

Controlled official26.3/protocol777, Linux Xvfb91/Mesa lavapipe: real chest pickup/place
obtained apples/bow/arrows; apple3->2, early60-second consume cancellation kept3, bow release
spawned an authoritative arrow (8->7 ammo, damage1), no-ammo use stayed inactive, slot/menu
release and reconnect passed. Inspected eating, release, inventory and reconnect output.
Ordinary/use horizontal velocities were approximately .117814/.023572 (about one-fifth),
with sprint disabled. Holding attack during use produced no mining/backlog; release resumed
ordinary mining and abort preserved the server block. Admin commands only prepared scene,
hunger, attributes and corrections; they did not perform these client interactions.
Evidence: executor `/workspace/recovery/java-item-use/` live-authority.log,
live-backlog.log, live-more-authority.log and retained JSON/screenshots; server
JAVA_ITEM_USE_ACCEPT markers in java-entity-use/server-console.log.

Remaining implementation: offhand/fallback and override-specific precedence, bow/charged
held-model predicates and full use animations/audio, crossbow/trident/kinetic uses, required
inventory/specialized menus, broader entities/combat/lifecycle and server UI/session work
in the queue above. Cancellation during a queued transport correction needs an explicit
cancellation-vs-position policy test. This is a connected implementation checkpoint, not
Bed Wars/Duels completion or iOS/online/Hypixel qualification. Full Cargo/MCP tests remain
incomplete; production-graph focused tests are narrower. A whole world-test wrapper lacked
its dev-feature dependencies; focused authoritative admission tests passed instead.

Out-of-height entry now retains real section publication receipts while allowing native void
gravity; only out-of-dimension cells become air, and missing in-range columns still wait.
Controlled Linux lavapipe vanilla26.3 accepted actual falling, server void damage/death, the
shared Respawn button, server-confirmed movement, inventory opening and reconnect. Respawn
retires old loaded/correction state. Neutral item-use cancellation is bounded/epoch-scoped and
waits for actual PlayerLoaded; its forced preloaded race has focused tests, not live coverage.
The fixture preserved items with keep_inventory during death and restored false afterward;
loot-drop behavior is unqualified. Full Cargo/MCP, iOS, online and Hypixel remain pending.

Native potion use now resolves PotionItem's conditional block PASS from authoritative block
tags, hit face, base potion registry and component patches. Water converts mud only without
custom effects; other admitted targets fall through to the existing drink/release owner.
Water/mundane/thick/awkward artwork now validates against the complete native potion registry
instead of the optional effect-color map. Linux vanilla26.3 lavapipe acceptance inspected real
chest transfers, water/swiftness drinks and server effects, mud conversion, custom-effect water
without conversion, long-drink cancellation, chest precedence and reconnect. No item/effect was
awarded locally. Potion-specific names/effect tooltip lines and iOS/online/Hypixel remain open.

Personal Java crafting inputs/output and armor cells now route existing shared UI hits into
the same native observed-click transactions. Recipe, equipment and curse rules remain on the
server; reserved slots never use ordinary storage merge prediction. Inspected Linux vanilla26.3
acceptance passed actual chest acquisition, one log→four planks with three logs retained,
helmet/chestplate equip and armorHUD8, rejected dirt→armor preserving eight, close/reopen and
reconnect. Slot icons/HUD are accepted; the avatar preview still lacks Java equipment.
Specialized workstations, recipe-book placement, armor dragging and iOS/Hypixel remain open.

The c04 implementation additionally passes a connected controlled offline flow with two actual
native clients: named chest acquisition→mine→place crafted plank→held eating→sword melee,
server damage/knockback→combat death→real Respawn→dimension switch→return/reconnect. Server
positions, health/counts and inspected screenshots support it. A keep_inventory fixture protected
items during death and was restored false; loot drops are unqualified. Slow MCP observations
missed the exact first apple count and overshot a chase; retained failures were resolved with
held-to-full count checks and ordinary attacks in a physical arena. This is not Hypixel/iOS
acceptance. Remote equipment is visibly absent (equipment packet remains deferred); title
component color is flattened, and Nether sky/clouds remain visually unqualified. These gaps
are not concealed by the working flow. Full Cargo/MCP suite remains incomplete.

Native remote equipment now decodes the real continuation/slot packet with the existing item
component reader and socket lifetime. Entity-incarnation storage feeds one bounded artwork
worker and the shared actor GPU pass, independent of personal inventory. The original offline
exporter adds eight native per-slot armor factories (42 total); old34 models and facts remain
unchanged, and the new report hash is verified. Inspected two-client Linux/lavapipe vanilla26.3
offline captures passed ordinary armor, leather dye/overlay, chainmail, boots, both held items,
glint, real helmet inventory removal/reequip, hotbar changes and crouch. Visibility unload/revisit,
dimension cleanup and reconnect passed with server state. `/workspace/recovery/java-remote-equipment/ACCEPTANCE.json`
retains screenshots, native pose oracle, focused tests and cold-capture/driver failures.
Trims, head/wing layers, active poses, custom skins, full resource-pack application/reload and
online/iOS/Hypixel acceptance remain incomplete; no new IPA exists. Full Cargo/MCP suite remains incomplete.

Native slash commands now use the existing chat editor/FIFO through the Java connection,
with session/epoch fences, retained backpressure and actual socket-write receipts. These
receipts do not mean that the server accepted a command. A normal-player vanilla trigger
fixture verified score7, then8, then9 after reconnect through real T/text/Enter input.
System components use the target language table, ordered/positional arguments and existing
bounded named styles; native heterogeneous NBT argument wrappers are preserved. Actual
formatted system text and action-bar display/expiry were inspected on Linux/lavapipe.
The native100-message history and separate coalesced overlay do not create another UI owner.
Player conversations and account-bound certificate/signature transport are integrated; the
controlled conversation fixtures are unsigned offline evidence. Commands now request native
server completions through the same connection. The shared editor applies the server's
UTF-16 replacement range, correlates edits/worlds, and supports Tab cycling and pointer
selection. Controlled vanilla verified root/argument suggestions, trigger scores13/17,
chat continuity and reconnect; four Linux/lavapipe screenshots were inspected. The first
capture preceded atmosphere readiness and does not qualify sky rendering or performance.
Ordinary message commands and aliases now parse the received native tree and share
player chat's signing/ACK chain. Linux offline self-whispers, a real counterpart's
receipt, trigger23, permission/tree replacement, root redirects, reconnect and native
configuration handoff passed. Four actual PNGs were inspected. Two concurrent GUI
attempts were incomplete under the container memory limit (OOM counter1→2); the
single-window sequence passed with no further OOM. Service-signed online commands
remain unqualified. Complex selectors, UUID targets, other unsupported signed prefixes,
string-mode admission hardening, suggestion tooltips and catalogs exceeding the existing editor's256-entry/64KiB budget remain incomplete; oversized lists
show a diagnostic. Underlined error context, full font/pack fidelity and authenticated,
iOS/Hypixel qualification remain open. Shared HUD regressions,
focused native-codec/text/input checks, production compilation and architecture pass; full
Cargo/MCP execution remains incomplete.

## World-drop integration — 2026-11-06

Q, Control+Q, configured remaps and Java touch utility choices now enter the same
ordered action owner as hotbar/use/attack. The socket checks inventory/selection,
world/correction, menu, readiness, death and spectator fences before writing.
Native successful world drops normally have no slot echo: the inventory owner
keeps a bounded provisional display separately from its last server snapshot,
updates it only after a local socket-write receipt, and replaces affected views
on authoritative updates. A receipt never means server acceptance.

Linux Xvfb/lavapipe + official offline vanilla26.3/protocol777 acceptance inspected
single/whole drops, slot ordering, chat focus, spectator guard, authoritative count
corrections and reconnect. Server NBT independently confirmed item counts. Native
merging turned the original ground stacks1+6 into7; the incorrect separate-6 fixture
assertion is retained, and an isolated real Control+Q test confirmed entity6/empty slot.
Evidence: `/workspace/recovery/java-world-drop/ACCEPTANCE.json`, associated PNGs,
`server-evidence.log` and `server-followup-evidence.log`. Native executable SHA256:
`e55688fa832829b5a9bdef65011e640a1e2fdb38c52be7aab87060dfd391e3b5`;
compiled c5d70506a + the exact30-path frozen source manifest in that record.
Focused checks passed:19 inventory,16 action-ordering,5 world admission,
11 interaction/fence and6 synthetic encrypted container transport tests.
Prior affected touch/input/settings evidence is retained separately. The full MCP
suite remains resource-incomplete; no new online, iOS or Hypixel acceptance or IPA.

Ground item visuals remain a real implementation gap: native ITEM_STACK metadata
and the shared world-item resource publication must be connected next. An admitted
drop/inventory count is not evidence that its item entity rendered.


Native dropped items now decode through the existing bounded item codec, retain scoped entity
appearance and render Ground-context geometry through the same atlas/rig publisher as remote
player equipment. Linux Xvfb/lavapipe official vanilla26.3 acceptance inspected real Q/Control-Q
items, count updates, server-authoritative pickup, remote gear coexistence/removal, tracking
unload/revisit, dimension replacement and reconnect. Server NBT independently confirmed stacks
and persisted item identity. Positions follow server snapshots; local item physics and pickup
animation remain incomplete. Full MCP tool tests pass16; this does not mean all application tests
ran. A live-found shared-pack defect is repaired: Bedrock pack publication preserves the
Java-owned terrain, block-entity, environment and actor resources. Actual application and
removal of an authored empty shared pack, unchanged native artwork, continued rendering and
server-confirmed pickup passed offline; this is edition isolation, not Java pack layering. No new online, iOS or Hypixel acceptance.


Native remote player swings now use the26.3 swing packet, socket lifetime, native hand/type/
duration and effect-adjusted visual timer. The existing player and equipment bones share the
sampled posture. Independent native wire, timer and30 pose vectors pass. Linux Xvfb/lavapipe
vanilla offline captures inspect ordinary six-tick main/offhand WHACK gestures, authoritative
attack damage and offhand placement, plus actor removal and reconnect. Equipment now retains
its committed same-body graphics during a forward update instead of briefly losing all armor
when weapon durability changes; removed/different-item slots hide immediately. Inspected
real-time recordings cover repeated damage and helmet removal/replacement/restoration. One
initial20fps recording dropped/repeated frames; the warm10fps retry, offhand and slot-change
clips did not. These are functional software-rendering observations, not frame-budget or iPhone
qualification. Local first-person input swings, STAB-specific held-item transforms, other living
schemas/use poses and full combat remain incomplete. No new online or Hypixel qualification.
