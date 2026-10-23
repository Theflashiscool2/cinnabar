# Workspace layout

How the client, the Go core and their libraries fit together, and what each crate and package owns. The crate layering rules are in `tools/architecture/policy.toml`.

## Overview

```text
cinnabar (Rust)  ── local socket ──  bedrock-core (Go, gophertunnel)
                                             ├─ go-raknet ──── servers and BDS
                                             └─ go-nethernet ─ Realms and friend worlds
```

Rust never implements Xbox authentication, encryption, RakNet or NetherNet; the core owns those
and relays packets over a local stream.

| Library | Used for |
| --- | --- |
| [protocolgen](https://github.com/cinnabar-mc/protocolgen) | Generates the Bedrock packet codec vendored in `crates/protocol/vendor`. |
| [Axolotl Stack](https://github.com/axolotl-stack/axolotl-stack) | Jolyne (client session library), whose raw packet entries and spawn sequence `crates/protocol` adapts. MIT; see `THIRD_PARTY_NOTICES.md`. |
| [gophertunnel](https://github.com/Sandertv/gophertunnel) | Bedrock login, encryption, resource packs and the packet relay. |
| [go-raknet](https://github.com/Sandertv/go-raknet) | RakNet transport to servers, plus server-list pings. |
| [go-nethernet](https://github.com/df-mc/go-nethernet) | WebRTC transport for Realms and friend worlds. |
| [go-xsapi](https://github.com/df-mc/go-xsapi) | Xbox Live identity, friends, presence and signaling. |
| [go-playfab](https://github.com/df-mc/go-playfab) | PlayFab sign-in and the menu catalog (featured servers). |
| [dragonfly](https://github.com/df-mc/dragonfly) | The built-in local-world server in `tools/localserver`. |

Mojang assets are never committed or embedded. `make assets` fetches Mojang's official
`bedrock-samples` pack (EULA-gated) and compiles it into carriers under the ignored `.local/`.

## Crate ownership

Every workspace member has one row below. The first column is its directory; the second is
its Cargo package name, which can differ from the directory. Import APIs from their owning
crate directly. The architecture gate checks these rows against `tools/architecture/policy.toml`,
which also requires every workspace member to be registered. Package names are checked against
their Cargo manifests. Each entry needs its package name, caller guidance and related owners.
Keep rows current when adding, moving or removing a crate.

### Client composition and adapters

Domain crates below hold the game rules and state without Bevy. The `client-*` adapters bind
those owners to the client frame and session boundaries. `client-state` holds shared ECS
contracts; systems live in the adapter that owns their work. `app` assembles the plugins and
coordinates work that crosses these owners.

| Workspace member | Package | Ownership and caller guidance | Related owners |
| --- | --- | --- | --- |
| `app` | `cinnabar` | Client executable and Bevy composition; use it to launch the client and assemble its plugins. | `client-*` crates own subsystem adapters; domain crates own game rules. |
| `web/client` | `web-client` | Browser page control, delivery admission and physical input translation. | `assets` owns carrier validation; `web-transport` owns relay sessions; `semantic-input` owns device frames; `cinnabar` owns application startup. |
| `web/transport` | `web-transport` | Browser WebTransport and WebSocket channels, bounded control and reconnect state; the browser host drives timers and passes prepared play I/O to the existing session. | `bridge` owns framing and the shared relay contract. |
| `packaging/android/client` | `cinnabar-android` | Android NativeActivity entry and setup-activity natives; build it with `packaging/android/build-apk.sh`. | `cinnabar` runs the client; `launcher` owns the Android install layout; `first-run` prepares carriers. |
| `crates/client-state` | `client-state` | Shared ECS resource wrappers, frame sets and observations; use these contracts to connect plugins. | Domain crates own transitions; the other `client-*` crates own systems. |
| `crates/client-runtime` | `client-runtime` | Transport resources, world servicing and session lifecycle adapters; use its runtime plugin and ingress/publication APIs. | `client-session` owns transport; `client-world` and `chunk-pipeline` own world work. |
| `crates/client-session` | `client-session` | Session transport, join bootstrap and separate bounded control/world FIFOs; use its network handle and sequenced events. | `pack-runtime` prepares packs; presentation owners interpret the opaque prepared payload. |
| `crates/client-shell` | `client-shell` | Menu input, launcher services and desktop settings; install its shell plugin. | `launcher-host` owns services; `launcher` owns models; screen crates draw the menus. |
| `crates/client-interaction` | `client-interaction` | Input-to-movement, camera and interaction adapters; install its interaction plugin. | `gameplay`, `sim` and `inventory` own decisions; `client-presentation` owns visual results. |
| `crates/client-ui` | `client-ui` | Retained client UI state, input routing and frame publication; call its UI runtime from client adapters. | `ui-screens`, `oreui` and `mod-ui` draw screen families; `ui-engine` retains their nodes. |
| `crates/client-presentation` | `client-presentation` | Camera, actor, equipment and environment frame preparation; use its plugin, environment state and borrowed observation inputs. | `actor-animation` evaluates poses; `render-model` defines CPU data; `render` submits it. |
| `crates/client-audio` | `client-audio` | Sound admission, playback and local audio cues; install its audio plugin. | `assets` supplies sound data; interaction and session adapters publish observations. |
| `crates/client-extensions` | `client-extensions` | Local mod and server Experience adapters, including consent input; install its extension plugin. | `mod-host` executes guests; `server-experience` negotiates them; `mod-ui` draws their surfaces. |

### Game state and input

| Workspace member | Package | Ownership and caller guidance | Related owners |
| --- | --- | --- | --- |
| `crates/world` | `world` | Palette-native chunks and world storage; Java terrain has explicit block/biome identities, dimension epochs and revision-scoped snapshots in `world::java`. | `client-world` admits network state; `meshing` produces geometry. |
| `crates/client-world` | `client-world` | Authoritative world, actor and local-player facts, decoding and ordered commits; use its ingestion and committed views. | `world` owns storage; `inventory` admits numeric session item facts; `chunk-pipeline` schedules terrain; `player-state` joins player authorities. |
| `crates/chunk-pipeline` | `chunk-pipeline` | Bounded terrain residency, worker scheduling and publication; use its world stream and publication snapshots. | `client-world` owns admitted state; `render-api` owns shared request bounds and receipts; the app encodes outbound requests through `protocol`; `meshing` builds geometry; `render` consumes publications. |
| `crates/far-terrain` | `far-terrain` | Far-terrain level of detail from received terrain: per-column surface summaries, their caches and bounds; use its summaries and config. | `chunk-pipeline` extracts summaries on its workers; `render` draws the far pass. |
| `crates/inventory` | `inventory` | Inventory authority, prediction, crafting and commands; use its session, ledger and router APIs. | `player-state` shares it with gameplay and UI; `protocol` defines wire requests. |
| `crates/player-state` | `player-state` | Shared inventory and local-player authority; use `PlayerState` to keep both on one session generation. | `inventory` owns stacks and crafting; `client-world` owns player facts. |
| `crates/gameplay` | `gameplay` | Local interaction decisions, prediction and ordered admission; use its mining, placement, item-use and movement APIs. | `sim` owns physics; `client-interaction` binds decisions to ECS; `client-session` sends packets. |
| `crates/sim` | `sim` | Deterministic movement, collision and prediction; use its simulator with caller-supplied world queries. | `gameplay` chooses actions; `client-interaction` drives ticks; `world` supplies block state. |
| `crates/input` | `semantic-input` | Device-independent actions, bindings and finalized input; use semantic snapshots instead of raw device events. | `client-shell` reads devices; `client-interaction` consumes actions; UI owns modal input routing. |
| `crates/env-compat` | `env-compat` | Reads `CINNABAR_*` environment variables, accepting their pre-rename `RUST_MCBE_*` spelling with a one-time warning; use its `var` and `var_os` for those variables. | Each variable's name stays a constant with its owner, such as `diagnostics::markers`. |
| `crates/simulation-clock` | `simulation-clock` | Shared simulation tick values and duration conversion; use its tick constants and conversion function. | Callers supply elapsed time; world and gameplay owners advance their own state. |
| `crates/block-transform` | `block-transform` | Position-dependent block offsets and rotations; use its transforms for consistent geometry and queries. | `assets` describes blocks; `meshing` and `sim` consume their transformed shapes. |
| `crates/actor-animation` | `actor-animation` | Actor rigs, Molang evaluation, controllers and poses; use its animation evaluation over actor observations. | `client-world` owns actor authority; `client-presentation` prepares frames. |
| `crates/particles` | `particles` | Particle definitions, emitters, collision and draw lists; use its simulation and effect library. | `assets` supplies pack definitions; client adapters trigger effects; `render` draws them. |

### Protocol and packs

| Workspace member | Package | Ownership and caller guidance | Related owners |
| --- | --- | --- | --- |
| `crates/bridge` | `bridge` | Framing and I/O for the local stream to the Go core; use its bounded bridge messages. | The Go core owns upstream transports; `protocol` interprets Bedrock packets. |
| `crates/protocol` | `protocol` | Bedrock packet codecs, shared wire contracts and session adapters; use its packet and codec APIs. | `bridge` owns local framing; `client-session` owns client transport; the codec lives in its vendored tree. |
| `crates/protocol-ids` | `protocol-ids` | Dependency-free named wire IDs and pinned air identities; import its constants and enums directly. | `protocol` and `assets` share these facts; the editor can consume them on WebAssembly. |
| `crates/java-protocol` | `java-protocol` | Native Java framing, discovery, authentication, login/configuration, terrain packet decoding and typed play facts; target comes from its own manifest; packet IDs generate from a pinned codec reference. | Independent of Bedrock `protocol` and `bridge`; launcher persists edition/accounts, `client-session` owns tasks and bounded user input, `client-world` admits generation-scoped facts into `world::java`; `assets::registry::java` verifies the pinned runtime block report/client archive and builds Java-native models; `meshing::java` and `chunk-pipeline::java` carry revision-scoped native snapshots into the shared renderer without Bedrock IDs. |
| `crates/pack-json` | `pack-json` | Shared pack text decoding and JSONC parsing; use its decoder before reading authored pack documents. | Pack admission, compilers and UI readers share this format policy. |
| `crates/zip-footer` | `zip-footer` | ZIP end-record admission for in-memory archives; run `prepare_zip_bytes` before handing bytes to a ZIP parser. | `resource-pack` and `shaderpack` share this archive policy. |
| `crates/resource-pack` | `resource-pack` | Bounded archive admission, decryption and pack-stack views; use its read-only admitted packs. | The Go core downloads packs; `pack-runtime` prepares them; the pack compiler owners compile them. |
| `crates/shaderpack` | `shaderpack` | Java Edition OptiFine/Iris shader-pack loading, configuration and translation; `ShaderPack` owns options, dimensions, property parsing, program discovery, enable rules and translation to validated naga modules with their binding reflection, while `PackSource` owns bounded source/asset reads and `discover_packs` lists installed packs. See its [contract](../../crates/shaderpack/README.md). | `render` creates pipelines from the modules; `app` lists installed packs for the Video renderer picker; `resource-pack` owns Bedrock packs. |
| `crates/assets` | `assets` | Pack source readers, compiled carrier formats and the pinned item- and entity-facts owner; use bounded loaders and typed asset data. | The pack compiler owners write carriers; `protocol-ids` supplies query metadata IDs; `pack-runtime` prepares active session stacks. |
| `crates/pack-compiler` | `pack-compiler` | Terrain, materials and independent carriers such as atmosphere, audio and UI; import its compiler APIs directly. | `pack-compile-input` owns shared inputs; `assets` owns formats. |
| `crates/pack-compiler/input` | `pack-compile-input` | Bounded pack sources, source validation, JSON and texture decoding; use its shared input APIs. | Terrain and entity compilers consume these inputs; `assets` owns formats. |
| `crates/pack-compiler/entity` | `pack-entity-compiler` | Entity, actor, equipment, icons, starter skins and block-entity artwork; import its compilation APIs directly. | `pack-compile-input` owns shared inputs; `pack-runtime` prepares session packs. |
| `crates/pack-compiler/font` | `pack-font-compiler` | Bitmap and outline font compilation, including native rasterization; use its font APIs from offline tools. | `assets` owns font carrier formats; `asset-compiler` owns the CLI. |
| `crates/asset-compiler` | `asset-compiler` | The `assetc` executable for runtime carrier preparation; use its CLI from asset setup. | the pack compiler owners implement compilation; `protocol-ids` validates generated event routes; `first-run` coordinates initial preparation. |
| `crates/pack-runtime` | `pack-runtime` | Runtime pack admission, compilation cache and reload dependencies; use its pack preparation and reload APIs. | `resource-pack` admits archives; `pack-entity-compiler` compiles entities; client adapters install the results. |

`assets` owns `crates/assets/data/item-facts-v2193.bin`, generated by `registryc` from
the pinned item tables and typed rules. It contains identifiers and behavior metadata,
with a checked schema and hash, rather than artwork or executable code. Regenerate it
through the item-facts compiler; runtime pack carriers remain local inputs.

`assets` also owns `crates/assets/data/entity-facts-v2193.bin`, generated by `registryc`
from pinned entity component metadata and typed compatibility rules. It contains
identifiers and constructor facts with a checked schema and hash. Runtime artwork
and pack carriers remain local inputs.

`assets::registry::java` owns native Java runtime block/model/texture compilation.
Its state catalog remains separate from shared GPU model/material resources.
Missing native graphics become typed pipeline dependencies. Worker-owned append-only
catalogs retain existing draw addresses; `assets::DrawResourceExtension` proves the
shared table prefix without exposing native IDs to the renderer. `ChunkTextureReload`
stages compatible GPU additions while preserving resident geometry. The application
waits for GPU readiness before installing the catalog, then only dependent sections
retry. Catalog identity and independent session/dimension/request fences reject stale work.
`assetc java-physics` exports native collision/surface facts from the verified pinned
server; `assets::registry::java::physics` validates the report before admission.
`assetc java-entities` exports hash-pinned local model hierarchies and checks native
entity-schema facts using the same verified offline runtime preparation. It does
not start an official client. `assets::entity::java` owns the native hierarchy
loader; model coordinates stay explicit until render-model admission. Exported
geometry and textures remain runtime assets, never source-controlled carriers.
`sim::java` owns native double-precision collision, separate from Bedrock arithmetic.
`sim::java::travel` owns dry-land tick math. `gameplay::java` borrows the single
world motion authority, tracks tick/send baselines and retains one pending outbound
frame. The application binds semantic input, native asset readiness and camera
presentation; it does not route Java prediction through Bedrock packets.
The Java domain store and bounded mesh pipeline fence session, dimension and neighbor
revisions; the application owns asset installation and render publication permits.
The world owner coalesces incremental block/light bounds within native sections;
meshing owns the dependency halo and the pipeline schedules intersecting sections.
Full column replacement/unload/biome changes remain distinct. Conservative column
revision fences retire stale jobs, which retry so initial terrain cannot disappear.
The world owner also samples native light storage for both terrain and actors.
Dimension skylight capability comes from the configuration registry; unallocated
layers use Java storage semantics while unreceived columns stay unavailable.
Light invalidation compares effective sources, including inherited bottom-row sky,
and identical or omitted layer updates preserve authority revisions.
Its Java presentation resource exposes GPU upload readiness to the shared loading
screen and sends Java's player-loaded command; Bedrock's stream/loading handshake
retains its own owner. Neither path fabricates the other edition's state.
`world::java::player` owns native position/velocity correction math; `client-world`
admits it before issuing a resolved teleport acknowledgement. The Java socket fences
the reply by dimension and correction receipt and preserves packet order while
waiting. Camera presentation observes the admitted correction without becoming a
second player authority.
`inventory::java` owns native storage, known/empty distinctions, component patches,
container state IDs and lifetimes. Local selection write receipts and server corrections
share its ordered admission path. `gameplay::java::actions` retains semantic hotbar and
attack order under backpressure; selection serialization receipts unblock subsequent
actions without inventing server acknowledgements. Native HUD snapshots carry the
selected slot separately from Bedrock inventory. `client-world` converts Java wire notifications
before admission; player and chest slot views reference the same storage. Native
item default semantics, transactions and UI presentation remain incomplete.
`assets::registry::java::interaction` owns runtime outline and interaction-shape data,
separately from collision facts. `sim::java::selection` owns native ray traversal;
`gameplay::java::interaction` retains scoped intent under transport backpressure.
The application binds finalized input to that owner and serializes native commands
through the Java protocol owner. Acknowledgements are processing receipts; only
ordered server updates alter terrain. Empty-hand creative breaking has Linux offline
fixture evidence; survival,
inventory-dependent behavior and entity targeting remain incomplete.
`world::java::entities::visual` owns native appearance facts, including explicit
incomplete metadata, registry-resolved pig variants and player clothing visibility. `client-world` validates
wire accessor/serializer pairs using hash-pinned generated schemas. Local player
metadata stays separate from remote entity lifetimes and follows respawn retention.
The protocol owner's original entity-facts report contains identifiers, dimensions,
update intervals, schema facts, default player skins and clothing masks; exported
model geometry stays outside the checkout. `world::java::roster` owns session-scoped
profiles and player-list facts, independently of dimension-scoped entity IDs. Entity
appearance retains an immutable profile until despawn. Unverified remote chat material
stays explicitly untrusted and does not authorize signed messages.
`render-model::actor::java_geometry` converts verified native hierarchies into the
common GPU rig contract, with a distinct catalog namespace. The application's native
entity adapter prepares assets off-frame and binds scoped domain facts to complete
`PreparedActorPublication` batches. Edition selection guards the shared actor catalog;
pose caches use session/dimension/incarnation lifetimes. Pig and default-skin player
bodies share rig publication, with separate body controllers and model poses. The domain
entity owner retains ordered visual targets separately from authoritative motion
and delta anchors; the host advances its clock and samples poses for publication.
Other entity renderers and full animation/teleport semantics remain incomplete.
`world::java::hud` owns native status snapshots; `client-world` resolves
wire game modes, experience and attributes before `client-ui` projects them into
its shared HUD store. Native mode visibility stays explicit, and projection rejects
retired sessions/dimensions. The application selects the edition's respawn queue.
`world::java::dynamics` owns admitted abilities and attribute arithmetic. The
`client-world` boundary resolves the hash-pinned protocol attribute IDs, isolates
local identity, and retains partial velocity without fabricating a position.
Angle-only corrections share the ordered acknowledgement fence; camera pose
revisions are distinct from teleport receipts. `world::java::entities` owns native
entity lifetimes and motion; `client-world::java::entities` admits decoded packet
facts with separate network IDs and scoped incarnations. Remote velocity targets
that store, while remote attributes and metadata remain incomplete. Generic
registry defaults never stand in for player attributes.
The target manifest pins original generated player-builder facts. Asset preparation
regenerates and verifies them from the pinned server; admission initializes them
before applying changed server attributes. Native dimension epochs also fence
held input and pending prediction independently of Bedrock dimension integers.
The movement adapter replaces camera angles from each new authoritative correction
before preparing another tick, because world publication follows network send.
Gameplay regression fixtures use `java-protocol` only as a development dependency
to exercise this receipt boundary through real admission.
The following legacy mapping is for Bedrock shader packs, not native Java admission.

`assets` also owns `crates/assets/data/java-mapping-v2193.bin`, generated by `registryc --kind
java-mapping` from the GeyserMC mappings and Geyser sources pinned in
`assets/geyser-mappings-source.json`. It maps block-registry states to Java block states, lists
Java block tags, the Java biome registry (climate values and sky, fog and water fog colour
attributes, in network registry order), the overworld, nether and end dimension types' colour
attributes and Java biome tags, and maps Bedrock biome ids to Java biomes and Bedrock entity
and item identifiers to Java entity types and items, for Java shader packs; its hash and
registry bindings are checked when it loads, and a carrier that fails them leaves everything
unmapped.

### UI layers

`ui` provides shared UI values and draw primitives. `json-ui` parses, resolves, binds and lays
out pack JSON, then emits draw commands. `ui-engine` adds screen registration, cached layout,
text and texture lookup, and retained nodes around that pipeline. These layers do not own
client session authority.

`ui-screens` draws vanilla JSON-UI screen families from borrowed inputs. `oreui` implements
the OreUI theme and screens in our own code using shared UI primitives. `mod-ui` draws mod
and Experience surfaces through `ui-engine`, including trusted consent chrome. `client-ui`
projects runtime state into those screen inputs, routes actions and publishes the retained UI
frame. The client adapters apply resulting commands to the domain owners; `render` submits
the prepared visual data to the GPU.

| Workspace member | Package | Ownership and caller guidance | Related owners |
| --- | --- | --- | --- |
| `crates/ui` | `ui` | Renderer-independent UI values, actions, primitives and text layout; use its models and draw lists. | `json-ui` handles pack syntax; screen crates compose controls; `render` draws outputs. |
| `crates/json-ui` | `json-ui` | JSON-UI parsing, inheritance, binding, layout and draw-command emission; use its catalog and control pipeline. | `ui-engine` adds retained screens and resources; `ui-screens` supplies screen-specific inputs. |
| `crates/ui-engine` | `ui-engine` | Portable screen engine, cached layout, painting and texture lookup; register screens and provide resource sources here. | `json-ui` resolves controls; `ui` owns primitives; screen crates own screen behavior. |
| `crates/ui-screens` | `ui-screens` | Vanilla JSON-UI screen families, HUD renderers and menu bindings; draw them with typed borrowed inputs. | `ui-engine` paints; `client-ui` owns runtime state; domain crates apply commands. |
| `crates/oreui` | `oreui` | OreUI theme, controls and screens drawn in our own code; use its screen and artwork APIs. | `ui` supplies primitives; `launcher` supplies menu models; `client-ui` routes runtime actions. |
| `crates/mod-ui` | `mod-ui` | Mod and Experience presentation, consent chrome, panels and HUD editor; draw surfaces from borrowed host inputs. | `ui-engine` paints; `mod-host` owns guest execution; `client-extensions` owns runtime integration. |

### Rendering contracts and backend

| Workspace member | Package | Ownership and caller guidance | Related owners |
| --- | --- | --- | --- |
| `crates/meshing` | `meshing` | CPU chunk, fluid, biome and cloud geometry; use its mesh builders over world and asset inputs. | `chunk-pipeline` schedules terrain work; `render` uploads the results. |
| `crates/render-api` | `render-api` | Dependency-free publication, pacing and skin-layout contracts; use these values across publication boundaries. | `render-model` owns richer CPU geometry; `render-scene` owns ECS submission resources. |
| `crates/cosmetics` | `cosmetics` | Bounded catalog metadata, admitted item handles and slot loadouts; resolve catalog IDs here before rendering. | `launcher-host` fetches verified assets; `render-model` bakes attachment geometry; the API owns entitlements. |
| `crates/render-model` | `render-model` | Engine-free render data and CPU geometry for actors, equipment and text; use its frame and geometry types. | `client-presentation` prepares data; `render-scene` carries submissions; `render` owns GPU work. |
| `crates/render-scene` | `render-scene` | Shared ECS render submission resources and frame acknowledgments; use these at the presentation/backend boundary. | `render-model` defines CPU data; `client-presentation` publishes; `render` consumes it. |
| `crates/render` | `render` | Bevy/wgpu terrain, actor and UI rendering; install its GPU plugins and submit prepared frames. | `meshing` builds CPU terrain; `client-presentation` prepares actors; `client-ui` prepares UI. |

### Launcher and services

| Workspace member | Package | Ownership and caller guidance | Related owners |
| --- | --- | --- | --- |
| `crates/launcher` | `launcher` | Bevy-free account, menu and world models plus product identity; use these models across services and screens. | `json-ui` owns vanilla screen names; `launcher-host` owns process and persistence services; `client-shell` integrates them. |
| `crates/launcher-host` | `launcher-host` | Launcher processes, platform services, persistence and background work; use its service APIs without Bevy. | `launcher` owns models; `resource-pack` validates user pack imports; the Go core owns authentication and upstream services; `client-shell` owns ECS adapters. |
| `crates/first-run` | `first-run` | Consent, download and preparation of local asset carriers, with the setup window; use its setup runner before launching the game window. | `asset-compiler` builds carriers; `bootstrap-ui` draws the setup window; `launcher-host` owns ordinary launcher services. |
| `crates/bootstrap-ui` | `bootstrap-ui` | The shared pre-game window stack for the setup and crash windows: panorama presenter, CPU-drawn OreUI canvas and controls, focus input, preferences, window attributes and the UI font lookup; build Bevy-free windows on it. | `first-run` owns the setup window; `crash-monitor` owns the crash window; `ui` owns the OreUI theme; `render-model` owns the panorama shader. |
| `crates/crash-monitor` | `crash-monitor` | The `--crash-monitor` process: minidump server watching the client, crash report folders, the crash window and native-dialog fallback; run it beside the client and show reports through its window API. | `crash-report` owns the report format; `launcher-host` installs in-process crash handlers; `bootstrap-ui` draws the window. |
| `crates/rich-presence` | `rich-presence` | Optional Discord desktop activity and RPC worker; use its presence and join-request APIs. | `client-shell` projects menu/session state; `launcher` owns product identity. |
| `crates/update-manifest` | `update-manifest` | Signed release manifest format, channels, verification and build comparison; use its manifest APIs for update decisions. | `release-manifest` signs releases; `launcher-host` downloads them; `update-installer` verifies staged payloads. |
| `crates/update-installer` | `update-installer` | Resumable verified files and the offline native apply helper; use its staging API for downloads and its executable after client exit. | `update-manifest` owns trust and ordering; `launcher-host` owns consent and helper handoff. |

### Extensions and examples

Extensions are disabled by default and do not change vanilla packet IDs or login. Guests use
the SDK for their own host contract; they do not import client runtime authority.

| Workspace member | Package | Ownership and caller guidance | Related owners |
| --- | --- | --- | --- |
| `crates/server-experience` | `server-experience` | Experience negotiation, bundles, cache and trust policy; use its verified session and bundle APIs. | `experience-runtime` runs the server guest; `client-extensions` hosts the client part. |
| `crates/experience-sdk` | `experience-sdk` | Server and client Experience guest SDKs and declaration generation; use the feature for the guest being built. | `experience-runtime` hosts server guests; `mod-host` hosts client components. |
| `crates/experience-runtime` | `experience-runtime` | One server Experience per process over framed stdin/stdout; use its runner and callback protocol. | `experience-sdk` defines guest bindings; `tools/localserver` provides server-side world callbacks. |
| `crates/mod-api` | `mod-api` | Personal-mod guest SDK generated from the extension WIT contract; use its bindings and `PlayerMod` API. | `mod-host` implements imports; Experience client parts use `experience-sdk` instead. |
| `crates/mod-host` | `mod-host` | Bounded component execution and explicit guest imports; use its host, grants and validated outputs. | `client-extensions` integrates guests; `mod-render` and `mod-ui` prepare their visual outputs. |
| `crates/mod-render` | `mod-render` | Validation of guest post passes and world primitives; use its validated geometry and shader outputs. | `mod-api` declares limits; `mod-host` accepts outputs; `render` submits them. |
| `examples/experiences/probe` | `experience-probe` | Server guest fixture for callbacks, world access and failure cases; build it for runtime contract tests. | `experience-sdk` supplies bindings; `experience-runtime` runs it. |
| `examples/experiences/terminal-client` | `experience-terminal-client` | Client guest fixture for terminal UI, dispatch and fuel failures; build it for host tests. | `experience-sdk` supplies client bindings; `mod-host` runs it. |
| `examples/mods/hello` | `hello-mod` | Minimal retained HUD label and keybind example; use it as a personal-mod template. | `mod-api` supplies imports; `mod-host` runs it. |
| `examples/mods/time-changer` | `time-changer-mod` | Local visual-time override example; build it to exercise host environment controls. | `mod-api` defines the imports; `client-extensions` applies the visual output. |
| `examples/mods/render-sample` | `render-sample-mod` | Post-pass and world-decal example; build it to exercise guest rendering. | `mod-render` validates outputs; `render` draws them. |
| `examples/mods/screen-probe` | `screen-probe-mod` | Player-mod screen fixture for events, data reads and traps; build it for host contract tests. | `mod-api` defines screens; `mod-host` executes callbacks; `mod-ui` draws them. |

### Diagnostics, tests and tools

| Workspace member | Package | Ownership and caller guidance | Related owners |
| --- | --- | --- | --- |
| `crates/diagnostics` | `diagnostics` | Runtime metrics, the process `tracing` subscriber and log categories, the ordered console writer, bounded per-session log files and shared diagnostic markers; install logging through it before anything logs. | `acceptance` evaluates evidence; subsystem owners produce observations; `app` adds Bevy-only profiler layers. |
| `crates/crash-report` | `crash-report` | Crash report schema, `crashes/<UTC time>-<id>/` folder layout, session log sections and retention; write, read, list and prune reports through it with caller-supplied time. | `launcher-host` installs the crash handlers that write reports; `devtool` bundles them; `diagnostics` owns the session identity. |
| `crates/acceptance` | `acceptance` | Optional acceptance evidence over committed runtime observations; use its control and witness adapters. | `diagnostics` owns marker contracts; world and render owners publish committed observations. |
| `crates/developer-control` | `developer-control` | Authenticated loopback control protocol, client/server and recording primitives; use it to drive developer builds. | `app` applies commands; `cinnabar-mcp` exposes them as tools. |
| `crates/test-time` | `test-time` | Test-only manual clocks and bounded eventual waits; use these instead of sleeping in tests. | Domain owners accept caller time; edge owners read real clocks. |
| `crates/test-alloc` | `test-alloc` | Per-thread allocation request counting for tests, benchmarks and opt-in runtime diagnostics; optional process totals compose with the chosen allocator. | Owning crates define allocation contracts for their own hot paths. |
| `tools/architecture` | `architecture` | Repository gate for boundaries, ownership docs, source limits and declarations; run its `check` command. | `policy.toml` defines rules; `workspace-declarations` shares metadata with `devtool`. |
| `tools/java-connect` | `java-connect` | Native Java saved-server and account diagnostics; use it to validate protocol handoff and keepalive/reconnect evidence without claiming rendering. | `java-protocol` owns wire/auth; `client-session` owns tasks; `client-world` admits facts; launcher owners persist servers/accounts. |
| `tools/devtool` | `devtool` | Affected verification, CI planning, captures, worktree tools and offline crash symbolication; use its commands for scoped checks and artifacts. | `architecture` enforces boundaries; `workspace-declarations` supplies metadata and path matching; `crash-report` owns the crash folders `symbolicate` reads. |
| `tools/workspace-declarations` | `workspace-declarations` | Shared Cargo metadata keys and repository path matching; use these declarations in gates and CI selection. | `architecture` validates metadata; `devtool` selects affected packages. |
| `tools/dist` | `dist-local` | Distribution staging, layout and hashes; use its CLI to assemble distributable bundles. | `launcher` owns install layout; `release-manifest` signs the published update manifest. |
| `tools/release-manifest` | `release-manifest` | Release key generation and manifest signing; use its CLI in release publishing. | `update-manifest` owns the format and verification; `dist-local` stages artifacts. |
| `tools/cxb` | `cinnabar-cxb` | Experience seeds, bundles, cache seeding and fixtures; use its publisher CLI. | `server-experience` owns formats, domains and limits; server tooling consumes its fixtures. |
| `tools/bedrock-replay` | `bedrock-replay` | Offline replay of saved server packets through a local session endpoint; use its replay CLI. | `bridge` owns framing; `protocol` owns packets; `client-session` consumes the replay. |
| `tools/jsonui-editor` | `jsonui-editor` | Shared JSON-UI editor API and browser frontend; use its editing, layout, rasterization and export APIs. | `ui-engine` owns screen resources; `json-ui` owns resolution; `jsonui-mcp` wraps the API. |
| `tools/jsonui-mcp` | `jsonui-mcp` | Stdio MCP frontend for JSON-UI editing and rendering; use its editor tools. | `jsonui-editor` owns editing operations; `mcp-stdio` owns the wire protocol. |
| `tools/ui-snapshot` | `ui-snapshot` | Named offline UI fixtures and PNG captures; use `--list` and `--state NAME --out PNG`. | `oreui` and `ui-screens` paint screens; `jsonui-editor` supplies the CPU target. |
| `tools/cinnabar-mcp` | `cinnabar-mcp` | Stdio MCP frontend that launches and drives a developer client; use its input, state, capture and recording tools. | `developer-control` owns loopback commands; `mcp-stdio` owns the wire protocol. |
| `tools/mcp-stdio` | `mcp-stdio` | Shared newline-delimited JSON-RPC MCP serving; implement its tool-server trait for a new stdio frontend. | `jsonui-mcp` and `cinnabar-mcp` own their tool definitions. |

| Go package (`core/`) | What it does |
| --- | --- |
| `cmd/bedrock-core` | The core binary. |
| `proxy` | Upstream session, resource-pack download and packet relay. |
| `authflow`, `authcache` | Microsoft device sign-in and token cache. |
| `catalog`, `launcher`, `control`, `xboxpresence` | Menu data and services: featured servers, Realms, friends and presence. |
| `localworld` | The launcher's open local world as the core routes it; `proxy` hosts it for Xbox friends. |
| `packcache` | On-disk cache of server packs. |

Cross-crate re-exports are forbidden: consumers import the owning crate directly.
Named exports must also have unambiguous ownership across Rust namespaces. Use
explicit dependency imports where external globs could supply an exported name.
The architecture gate also rejects glob re-exports with restricted visibility and
executable file headers regardless of extension. Files marked `binary` in Git
need a named ownership record; executable files cannot use that exemption.
Temporary, individually named protocol and session forwarding APIs are tracked
in `tools/architecture/policy.toml` for removal with #532.
