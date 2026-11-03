# Native Java server feature integration

Target: Cinnabar on iPhone, with desktop controlled-server checks first. Java 26.3,
protocol 777. This inventory compares source, not marketing claims or runtime qualification.
Reference baseline: `Vasysik/horizon-android` commit
`844ef35f2b14e3fed99d5d2e05b274193fae4388` (MIT). Local reference-only desktop helpers
at `5b8e6d0b6453e57fb23eb7bbb365bbc7edd5b9b2` do not become Cinnabar product code.

`L` = reference `crates/horizon-client/src`, `G` = `L/game`,
`R` = `crates/horizon-client-render/src`, `W` = `crates/horizon-world/src`.
Inventory/menu input, item-model and hash rows are backed by targeted source inspection.
Other areas are an index from the reference's `docs/architecture.md` and source paths;
their detailed behavior and harnesses still need assessment before a port is called complete.
Reference harnesses were not rerun here. A screen enum, codec, or passing structural test
is not end-to-end acceptance.

| Capability / reference owners | Cinnabar owner | Integration state and next required evidence |
|---|---|---|
| Native login/configuration/reconfiguration/transfers (`G/remote.rs`, `L/net.rs`, protocol packets) | `java-protocol/session`, `client-world/java`, `launcher-host/java` | Existing native path. Owner has authenticated Hypixel lobby evidence; repeated backend transition/registry compatibility still requires controlled regression. |
| Microsoft accounts, refresh/persistence, server selection (`launcher/src`, `L/title`) | `launcher/java_accounts`, platform Keychain, `client-shell` | Existing accepted owner login/discovery. Preserve; no re-registration required. Agent has no reusable online account. |
| Player inventory, hotbar, cursor, pickups (`L/menu.rs`, `L/container_screen.rs`, `G/remote.rs`) | `inventory/java`, `client-world/java/inventory`, `client-ui/ui_runtime/java_inventory`, app binding | **Implemented and Linux offline-live tested**: native contents/slot aliases/revisions now feed shared JSON-UI. Player storage/hotbar/equipment/cursor counts, ordinary pickup/place/split/merge and authoritative updates passed. iOS and online acceptance pending. |
| Split, shift-click, drag, swap, clone, throw, double-click (`L/container_screen.rs`, `L/menu.rs`) | Native pickup planner + outbound | Ordinary left/right pickups implemented; remaining gestures need native planning/codecs and real server reconciliation. Unsupported gestures refuse instead of mutating Bedrock inventory. |
| Component hashes/custom names/lore/glint/durability (`horizon-protocol/item/hash`, `hash_ops`, `R/containers.rs`) | `java-protocol/session/inventory`, native UI | CRC32C/HashOps primitives ported under MIT; common scalar/text hashes adapted. Unported hash codecs explicitly require server resync. Names/artwork/tooltip completeness remains pending. |
| Client item model trees/generated extrusion/transforms/special items (`horizon-assets/item_model.rs`, `item_bake.rs`, `R/model_library.rs`) | Native Java assets + shared item renderer | **Gap**: shared default atlas only matches canonical names. Custom Java item models, dynamic select/range/condition, composites, special item artwork need native resource resolution. Do not claim exact item-model parity. |
| Chest/double chest/server chest menus (`G/remote.rs`, `R/containers.rs`) | Native inventory + existing JSON-UI | 27/54 layouts connected; native single/double chest GUI transfers, server-localized titles, custom-name/glint items, full-slot refusal and cleanup passed Linux vanilla26.3 offline acceptance. Other 9x row counts and specialized layouts remain unsupported. |
| Crafting/recipe book/ghost recipes (`L/crafting.rs`, `L/recipe_book*`, `G/machines.rs`) | Shared JSON-UI; Java data adapter missing | Native player crafting/output rules and server recipes not connected. Bedrock crafting does not prove Java support. |
| Furnace/smoker/blast furnace, brewing, hopper/dispenser/dropper/shulker (`L/stations.rs`, `G/remote_stations.rs`) | Existing shared screens; native authority | Layout/property/action adapters pending. Native unsupported menu admission remains explicit. |
| Anvil/smithing/grindstone/stonecutter/loom/cartography/enchanting/beacon/crafter/lectern (`L/stations.rs`, `R/containers.rs`) | Existing shared screens; native adapters | Specialized rules/output/button/name/property packets missing or unqualified. Reuse UI; never reinterpret native slots as Bedrock UI slot IDs. |
| Merchant offers/trading, mount equipment (`L/merchant.rs`, `G/trading.rs`, `G/remote_vehicles.rs`) | Native session + shared UI | Additional native packets/state/screens/interactions required. Local reference trade/AI simulation is not needed to consume authoritative remote offers. |
| Remote players/skins/equipment/poses/labels (`G/remote.rs`, `G/player_equip.rs`, `R/entity*`, `R/tab_list.rs`) | `client-world/java/entities`, `app/session/java_entities`, shared nametags | Partial native presentation exists; verify missing owner-observed players/labels through packet→metadata→extraction path. Not a loading-success claim. |
| Mobs, projectiles, dropped items, vehicles/passengers, armor stands/displays (`G/remote_*`, `L/mobs/render.rs`) | Native entity storage/render extraction | Per-type metadata/visual/action coverage incomplete; identity reuse/removal and visible authoritative tests required. |
| Walking/jumping/sneak/sprint/fluid/climbing/scaffolding/lava/corrections (`L/motion`, `W/player_travel`) | `gameplay/java`, `sim/java`, app movement | Existing controlled offline evidence; preserve. Additional abilities/effects/velocity/mount/latency scope remains to qualify. |
| Mining/placement/item use/food/bows/buckets (`G/hand.rs`, `G/consume.rs`, item rules) | `gameplay/java/actions`, native interaction | Existing creative/main-hand path; survival/tool/use/offhand scope incomplete. Server commands are setup, not proof of client action. |
| Combat/shields/ranged/health/death/respawn/effects (`G/remote.rs`, `G/hand.rs`) | Native gameplay/player HUD | Partial player status/respawn; attack/entity targeting and full action lifecycle need authoritative gameplay acceptance. |
| Chat/signing/commands/suggestions (`G/remote_chat.rs`, `G/chat.rs`, `R/chat.rs`) | Native protocol + existing chat UI | General Java chat/command/signing/ack integration still incomplete; don't fake signatures or mark local server command parser as remote client support. |
| Sidebar/teams/tab/list/below-name/titles/boss/actionbar (`R/hud*`, `R/tab_list.rs`, `G/remote.rs`) | `client-world/java/scoreboard`, shared HUD/nametags | Sidebar/teams accepted at clean `334c4fd2e`; preserve. Other HUD surfaces must be assessed independently; full translation/RGB/custom-font formatting remains a gap. |
| Chunk/light/biomes/fluids/block entities/pistons (`R/world.rs`, `R/light.rs`, `G/remote_chunks.rs`) | Native Java terrain + existing shared GPU renderer | Existing audit/controlled evidence preserved; no terrain restart. Feature audit success does not prove server gameplay or all pack behavior. |
| Local/server packs, fonts, item components/resources (`R/packs`, `G/packs.rs`, assets) | Native resources + shared reload | Local terrain reload accepted within declared scope. Full Java item/font/offer/status/application pipeline incomplete. Bound resource downloads and retire old work across reload. |
| Sounds/particles/weather/dimensions/time/fog/sky (`R/particles.rs`, `G/level_particles.rs`, `G/remote.rs`) | Native world + existing shared atmosphere/audio | Existing weather/End/light evidence; official rain audio externally unavailable. Essential remote gameplay sounds/particles still need coverage. |
| Keyboard/mouse/touch/controller/accessibility/settings/capture (`horizon-client-input`, `R/options*`) | Cinnabar input/settings/capture/iOS host | Preserve existing iOS scene/touch/platform work. Android paths cannot establish iOS functionality. Native Java menus must use existing touch/pointer/text routing. |
| Tick/render cadence/streaming/culling/performance (`L/clock.rs`, `R/gfx`, streaming) | Existing Cinnabar scheduling/renderer | Reuse existing owners; profile demonstrated native bottlenecks after correct gameplay. Software rendering is functional evidence only. |
| World save/generation/spawning/mob AI/villages/raids/redstone/commands/TNT/boss AI/LAN/integrated/dedicated server (`W/worldgen`, `L/mobs`, `G/lan*`, `G/levels`, `horizon-server`) | Separate local-world/server scope | Reference has implementation and many fixtures. These are server-side simulation, not prerequisites for remote Hypixel play. Not copied or falsely marked Cinnabar complete; retain as separate substantial scope. |
| Friends/hosting/agents/benchmarks/telemetry/mod-porting/shaders/release CI (`L/control`, `horizon-port-pack`, telemetry, workflows) | Cinnabar platform/extension tools | Separate optional/service/tooling scope. Not evidence of vanilla protocol compatibility or iPhone playability. No upstream communication/publication or service provisioning authorized. |

Reference has a larger implementation, not a drop-in Cinnabar library. Copy compatible MIT
algorithms with attribution; adapt protocol/domain/resource/input boundaries. Never embed a
second client or use the reference's local-server simulation as remote server authority.

Inventory evidence: domain9, wire/hash15, admission3, shared-screen18 and native UI/HUD11
regressions pass; one encrypted synthetic-wire reused-menu/close-ordering fixture passes
separately. Actual Linux Xvfb/lavapipe vanilla26.3 offline GUI tests exercised player pickup,
bread11 split/merge, chest transfers, custom components, full64-slot refusal, double chest
slot53, kick/menu cleanup, same-process offline reconnect and unload/revisit. Fixed a live
stale-cursor defect by sending the unchanged client view rather than claiming unapplied
predictions; vanilla now supplies authoritative deltas. Final candidate titles Chest/Large
Chest and reconnect with retained items were inspected. Resource reload was not established.
Mac, simulator, physical iPhone, authenticated vanilla and Hypixel remain unattempted by
the agent; compilation or prior owner lobby access does not qualify this inventory change.

The companion [handler inventory](java-server-handler-inventory.json) records all143 pinned
play clientbound packets,122 source match-arm variants in the reference (including explicit
unknown/debug handling), source locations and15 actual deferred types from this controlled
offline run. It is a discovery index, not a support assertion: a named guard or ignored arm
is not an implemented feature, and production delegation must be traced before assigning a gap.
