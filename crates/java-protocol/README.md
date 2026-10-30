# Native Java connection foundation

This crate owns Java wire framing, discovery, authentication and the ordered
login/configuration/play connection. It uses native TCP, independently of the
Bedrock Go core. Native terrain now reaches the existing Cinnabar renderer through
the owners below; a playable Java client remains unimplemented.

The exact supported release and protocol, official server checksum, and pinned
protocol-data revisions live in [data/target.json](data/target.json).
`build.rs` derives state-specific packet tables from the pinned, MIT-licensed
`data/MinecraftCodec.java` reference and checks its version against the manifest.
That file is build input only, never compiled or run as a Java client.
`Cargo.lock` pins Rust dependencies. The launcher always uses this implemented
protocol. An advertised mismatch is unknown compatibility, since proxies can
accept multiple versions; a status response alone never establishes support.
Launcher joins do not require status discovery to succeed. Actual login rejects
are reported by the native session. The acceptance tool retains its strict
version qualification checks.

## Integration and exercise

Saved servers without `edition` remain Bedrock. In Add Server, select Java
Edition. An empty port means Java's default and permits SRV discovery; an
explicit port suppresses SRV. The original hostname remains the handshake
identity while DNS/SRV selects the socket destination. Java discovery runs on a
cancellable worker; description text is bounded plain text, never executable.
Favicons are retained only as bounded status data and never fetched or rendered.
A valid status survives a missing pong with unknown latency. A malformed or
mismatched pong is an error. Offline, timeout and malformed replies stay distinct.

The app routes Java joins to `client-session::java::Session`, which owns a
bounded event FIFO and abortable task. Retirement cancels preparation and socket
work; `client-world::java::WorldState` rejects old generations before mutation.
The launcher shows actual protocol state and bootstrap facts while native terrain
loads, then shows server terrain after the spawn upload reaches the GPU. Cancel, retry and server switching
use the existing session lifecycle. Java credentials have their own namespace;
Bedrock credentials and transports are never reused as Java identities.

Build the developer acceptance path:

```sh
cargo build -p java-connect
cargo test -p java-protocol
cargo run -p java-connect -- --servers /path/to/servers.json --index 0 \
  --accounts /path/to/java-account-directory --keepalives 2 --reconnect 2
```

The saved-server file is the existing launcher JSON array, for example:

```json
[{"name":"Local Java fixture","address":"127.0.0.1:25567","edition":"java"}]
```

For an explicitly authorized, local **offline-mode test fixture only**, replace
`--accounts ...` with `--offline-fixture NativeFixture`. Every resolved destination
must be loopback. The tool requires bootstrap, position and actual received and
answered keepalives in each generation; a timer is only a failure deadline.
It reports typed JSON facts, disconnects, and repeats with a new generation.
It never downloads a server or accepts its EULA. Obtain the official server
identified by the manifest, verify its checksum, accept the EULA yourself, and
run it with the manifest's required Java runtime. Bind a fixture to loopback;
use a whitelist and add the fixture identity. Do not change a production server's
online-mode setting. Server binaries, assets and raw captures stay outside Git.

`java-status HOST [PORT] [PROTOCOL]` remains a low-level status probe with strict
pong semantics; the launcher uses the more tolerant `transport::discover` API.

## Authentication

Register this fork's own Microsoft public-client application, enable the device
code flow for personal accounts, and obtain Minecraft service approval. Device
flow has no redirect URI. No application is supplied or borrowed by this fork.
The user's environment currently has no application, so real online-mode
acceptance remains blocked.

```sh
cargo run -p java-connect -- account-login /path/to/java-account-directory PUBLIC_CLIENT_ID
cargo run -p java-connect -- account-select /path/to/java-account-directory PROFILE_UUID
cargo run -p java-connect -- account-logout /path/to/java-account-directory PROFILE_UUID
```

For the graphical client, use the `java` sibling directory of its Bedrock auth
cache. Sign-in writes public `oauth-client.json` there and stores tokens with the
existing protected, atomic account-file helpers. Unix files are mode 0600.
The flow handles device polling, decline, expiry, cancellation, refresh, Xbox user
and Java XSTS exchange, Java entitlement/profile checks, and server session join.
An active graphical or diagnostic session closes when its selected Java account
is switched or logged out; the account monitor checks once per second. No credentials or service response bodies are
included in errors or diagnostic output. Device prompts necessarily display the
short-lived user code; do not publish that output.

The official device-flow contract was checked against Microsoft's
[documentation](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code),
revision `a4be4ac419c4e857b1c4de7dee22c9f7e0c750f9`, on 2026-10-22.
Minecraft exchanges were cross-checked with the pinned Azalea implementation
listed below. Live application approval, entitlement and online encryption/session
join evidence remain required; fake services do not establish service acceptance.

## Wire and admission contract

One future owns socket reads and writes, preserving compression/encryption
transition ordering and continuous AES-CFB8 state. Three-byte frame lengths,
8 MiB inflated packets, exact zlib completion, a 64 MiB configuration budget,
and bounded NBT depth/node counts prevent unbounded allocation. These are
explicit admission limits, not a claim of full vanilla/custom-pack parity.
Connect and login/configuration have separate deadlines; play has an idle-read
limit instead of a total login deadline. Cancellation drops the stream.

Configuration retains registries, tags and features, requests complete registry
values by accepting no known packs, responds to keepalives/pings/cookies and
acknowledges completion. Play publishes the authenticated profile, server-session
UUID, authoritative entity/world/dimension identities and registry facts, then
position, death, health/food and respawn facts. Explicit respawn input uses a bounded
connection-owned queue; interrupted reads retain prefix/body/cipher progress. It answers
teleports, keepalives, pings and chunk-batch flow control. Terrain packets decode off
the rendering thread. Entity lifecycle and selected metadata have domain admission;
inventory contents, slot/cursor/selection updates and container lifetimes now have
an edition-owned admission path with the limited live evidence below;
the renderer receives no synthetic Bedrock packets or fabricated world.

Unsupported required resource packs, experimental features, transfer, consent
or interactive dialogs, an initial relative teleport without an admitted anchor,
mid-session reconfiguration and tag replacement fail
clearly. Inventory interaction/presentation and chat signing remain incomplete. Native
movement, entity and terrain presentation have the limited offline rendered
evidence below; complete world and gameplay acceptance remain open. No modded-server or proxy compatibility is claimed. Unknown state packet IDs are protocol errors.

`world::java::player::Motion` owns double-precision position/velocity and correction
math. `client-world::java` resolves absolute/relative corrections against that current
authority, including rotated relative velocity. The socket pauses ordered packet
admission until the resolved pose is returned through its bounded command queue;
it never acknowledges raw relative coordinates. Session, dimension and a monotonic
correction receipt fence stale replies, including reused server teleport IDs.
An unadmitted correction has a ten-second deadline; cancellation still closes the
transport. This deadline is an explicit implementation budget, not a vanilla parity
claim. The application retains a full-queue reply for a later frame and applies each
new correction receipt to the camera. The dry-land movement slice below adds
physics and outbound movement; other movement semantics remain incomplete. Deterministic coverage checks encrypted wire ordering, cancellation,
timeouts, correction precision and stale receipts.

Player abilities, attributes and packed `LpVec3` velocity now have typed decoders and
local-player admission. The attribute ID table is generated from the hash-pinned
`AttributeType.java` reference; all 40 IDs were checked against the official registry
report whose hash is recorded in the target manifest. Generic attribute defaults
are not substituted for server-supplied player values. Remote velocity and movement now target scoped entity lifetimes; remote attributes
remain deferred. Velocity before initial
position does not invent a spawn anchor. Respawn retains attribute bases and obeys
the server's modifier-retention flag. Local metadata follows respawn retention;
pig/player/item schemas are admitted with the explicit complex-value gaps below.

Angle-only corrections use the same bounded, ordered admission deadline and receipt
fence as teleports. Their resolved rotation is returned in Java's rotation packet;
camera updates use a domain pose revision independent of teleport IDs. Independent
vanilla velocity vectors, malformed payloads, encrypted rotation-response ordering,
attribute arithmetic, admission atomicity and stale identities have deterministic
coverage. This does not establish client walking or general entity synchronization.
Linux offline vanilla diagnostics also admitted absolute rotation `90/25`, relative
`+15/-10`, and a return to `0/0`; server entity data confirmed `90/25` and `105/15`
after the replies. That run retained 49 columns, positive health and four real
keepalives before clean disconnect. This particular run is diagnostic evidence; the later graphical correction and
walking acceptance is described below.
The server sent three changed player attributes, not the complete player defaults.
The initial values now come from the pinned vanilla player builder's generated
report, with server updates replacing them. A direct vanilla registry comparison confirmed all
codec IDs and bounds, but found a rounded jump-strength default in the reference;
generic defaults are therefore deliberately absent from the generated runtime table.

Linux offline vanilla acceptance on 2026-10-22 also exercised real corrections:
the diagnostic client admitted two relative teleports (wire flags `255`), an
absolute return, 49 columns and three keepalives. In the headless graphical client,
the saved Java row reached the rendered world; a server relative teleport changed
feet from `[6.5, -60, 10.5]` to `[7.5, -60, 12.5]` with yaw/pitch `90/25`.
Admitted facts, camera and server entity data agreed, and fresh before/after frames
were inspected at 1280×720, scale 1, Vulkan lavapipe. An absolute return also replaced
the camera pose; seven real keepalives were observed before Save & Quit cleared
the native presentation and the server reported zero players. This is server
correction evidence, not local movement physics, entity rendering or gameplay.
Rejoining in the same client advanced generation `2→4`, reset the correction
receipt to the new connection, rendered 49 fresh columns and answered two more
keepalives without restoring the retired pose.

## Terrain admission and native graphics (experimental)

`terrain` follows the manifest-pinned MCProtocolLib `MinecraftTypes`,
`ClientboundLevelChunkWithLightPacket`, `ClientboundSectionBlocksUpdatePacket`,
`ClientboundChunksBiomesPacket` and `ClientboundLightUpdatePacket` schemas.
The selected release has block **and fluid** counts, fixed-size palette long arrays
without length prefixes, and byte-array light masks. Heightmaps are typed long
arrays, not the older NBT heightmap compound. Network block-entity NBT remains typed.

`client-world::java` admits those packets into `world::java::Terrain`: Java-only
block/biome IDs, compact Y/Z/X palettes, full columns, light/biome updates, block
changes and unloads. Negative coordinates use Euclidean chunk division. Missing
columns remain unknown, not air or fullbright. `world::java::Light` samples resident
light storage for meshes and actors: absent block layers read zero; absent sky
layers inherit the bottom row of the next stored layer above, or full sky above
all stored layers. Explicit empty layers remain zero. The native dimension registry
provides `has_skylight`; dimensions without sky read zero sky light. Incremental
updates retain omitted layers and invalidate publication revisions. The reference
is the manifest-pinned client archive’s `BlockLightSectionStorage.getLightValue`
and `SkyLightSectionStorage.getLightValue(..., false)`, checked with an isolated
original probe and independent numerical vectors. No official client is launched.
Session/dimension epochs and
column/neighbour revisions reject obsolete publications. Respawn clears terrain
and position before admitting the new world. Nonresident incremental updates do
not invent columns and are counted. Block replacement invalidates old block-entity
facts and heightmaps; incremental heightmap rebuilding remains unimplemented.

The official server's `net.minecraft.data.Main --reports` generates `reports/blocks.json`.
Its SHA-256, byte count and state count are pinned in `data/target.json`.
`assets::registry::java::BlockRegistry` verifies that report before admitting identities,
defaults and properties. It is **not** a collision/model registry. Keep the generated
report and game jar outside git. After obtaining the pinned server under its EULA:

```sh
java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports --output REPORT_DIR
cargo build --locked -p java-connect
# Use the actual Java row in a saved-server file; fixture identities are loopback-only.
target/debug/java-connect --servers SERVERS_JSON --index 0 \
  --offline-fixture NativeFixture --block-report REPORT_DIR/reports/blocks.json \
  --inspect-block -5,-61,-8 --require-chunks 1 --keepalives 2 --reconnect 2
```

Pass `--inspect-block` and its comma-separated position as separate arguments.
`--respawn-on-death` explicitly requests server respawn for the diagnostic fixture;
it is not enabled by default. Acceptance requires live terrain, a position,
positive authoritative health and the requested real keepalive count. Reported
terrain payload bytes exclude allocator overhead, NBT, registries and renderer data;
they are not a process-memory measurement. Provisional dimension/admission budgets
still require custom-dimension qualification. No mesh, collision, movement, model,
texture or graphical-world acceptance is implied by this diagnostic.

Live Linux offline evidence on 2026-10-22: a saved dead player respawned, entered
terrain epoch 2 with 49 columns and health 20, and a second session independently
admitted 49 columns. Each answered two real keepalives. A separate four-keepalive
run admitted four block updates, including a console dirt placement and subsequent
server grass spread; the sampled state resolved to `minecraft:grass_block`,
`snowy=false`. Those diagnostic runs exercised no graphical world or input loop. Online mode still
requires this fork's OAuth application and legitimate user sign-in.

`target.json` also pins the official client archive's SHA-1, SHA-256 and byte length.
`assets::registry::java` reads its ZIP entries without extracting paths, resolves
variants/multipart selectors and weighted positional choices, model inheritance,
texture indirection, cuboid geometry, XYZ/model-element rotations and UV lock.
It compiles real PNGs, uniform animation timelines and alpha materials into the
existing graphics tables. Java state IDs stay in a separate native catalog; no
Bedrock visual/runtime IDs or Geyser mappings are used. Runtime resources stay
outside source control. Parsing budgets and unsupported material/tint/animation
semantics produce explicit errors instead of invisible blocks.

Behavior references are the pinned client's `Variant`, `MultiPartModel`,
`TextureSlots`, `CuboidModelElement`, `FaceBakery`, `BlockColors`,
`AnimationMetadataSection`, `BiomeSpecialEffects` and `ColorMapColorUtil` classes.
The Rust implementation is original; game class/source files are not distributed.
Smooth Java AO/light interpolation, fluids, partial-face occlusion, nonuniform
biome blending, spatial tint callbacks, per-face UV-region transparency classification,
variable-duration texture animation and server clock/weather presentation remain
incomplete. Initial graphics preparation collects resident states. Subsequently missing
states are typed mesh dependencies, not fatal errors or air. A cancellable worker
compiles only new state models and appends stable template/material/animation addresses;
source pixels are retained to rebuild texture pages without rereading old sprites.
An asset-owned prefix proof permits the shared renderer to stage and publish new GPU
tables while keeping resident geometry. Only waiting sections retry; old jobs remain
valid against the retained address prefix. Session/dimension and exact catalog identity
fence CPU results; the GPU mailbox independently fences its world and request lifetime.
Unsupported model/tint semantics still fail explicitly. Runtime archives and block
reports are revalidated on each extension worker. Texture-page rebuilding/upload and
retained catalog memory have not met the project's performance qualification budgets.

Linux offline vanilla acceptance (hidden Vulkan/lavapipe, 1280×720, scale 1) introduces
command-block, gold-block and oak-log states after launcher join. Three extensions
publish without losing existing terrain; a subsequent log-axis change visibly selects
its end texture. Actual empty-hand creative input removes the new gold block, while
the non-operator command-block attempt is acknowledged and rejected by server authority.
Fresh frames and server queries confirm both outcomes. Twelve keepalives precede cleanup.
Reconnect advances generation 2→4 with no inherited actions or extension state, then
renders another log variant. After seven keepalives, Save & Quit cancels a newly observed
pending gold-state extension and 206 outstanding mesh jobs. Generation 6 renders the
server's resulting world, retains one pig and answers five keepalives before cleanup.
No render/protocol error is observed. Native render/interaction records clear on each
exit; the fixture has no established client TCP connection afterward. Procfs prevents
per-process socket ownership inspection. Debug/software-renderer RSS samples around
2.1 GiB are not memory or frame-budget qualification.

The missing-graphics regression fails before the fix and passes afterward. Native
owner tests, resource-prefix rejection, stale-world/catalog fences, compatible GPU
publication, existing reload regressions, app cancellation/readiness tests, clippy,
formatting, architecture and the graphical build pass. Full-workspace merge gates,
Windows/macOS and online-mode acceptance remain unrun.

Incremental terrain changes now carry section-local block/light bounds. Meshing owns
its one-cell dependency halo; the pipeline queues only intersecting resident sections.
Full column replacements, unloads and biome updates retain full-column invalidation.
Light changes compare effective sources, including inherited bottom-row skylight;
identical or omitted updates preserve revisions. Column revision fences remain
conservative: obsolete in-flight jobs retry rather than losing their initial mesh.

The same Linux fixture command-block addition dispatches two jobs (including its
missing-graphics retry), versus 217 before spatial invalidation. An interior dirt
update and actual creative break each dispatch one job. Breaking at a chunk boundary
dispatches two jobs, and a fresh frame shows the newly exposed neighbouring face;
server queries confirm the removed and retained blocks. Reconnect advances generation
2→4, renders the remaining server block, clears action counters and observes four
further keepalives before fixture restoration and disconnect. Focused world/meshing/pipeline
regressions, scoped clippy, formatting, architecture and the graphical build pass.
These are measured work counts, not frame-rate or edit-latency qualification. Effective
light-source changes still conservatively invalidate whole light sections. General
playable support and the project's performance budgets remain incomplete.

`meshing::java` consumes immutable domain snapshots and produces shared Cinnabar
mesh buffers, sampling the server's separate sky/block light nibbles.
`chunk-pipeline::java` bounds concurrent jobs/results, carries neighbor revision
keys and rejects obsolete work. Unreceived neighbor columns remain pending until
new authority arrives. Its uniform-biome path preserves the actual native registry index and
fails explicitly when Java Voronoi/blend processing is required.

For CPU mesh acceptance, add `--client-assets CLIENT_JAR --compile-terrain` to the
command above. Linux offline vanilla acceptance compiled five real state identities
into seven materials, thirteen templates and 521 visible model quads across 24
center-column sections; the sampled oak stair produced eleven source model quads.
That run observed 49 columns and two real keepalives. `rendered_world` stayed false.

The graphical experimental path loads `client.jar`, `blocks.json`, `physics.bin` and `interaction.bin` from
`CINNABAR_JAVA_ASSETS` (default `.local/assets/java/<pinned-version>`).
Build with `cargo build --locked -p cinnabar --features developer-control` and
launch through the repository's headless client MCP. Developer builds alone accept
`CINNABAR_JAVA_OFFLINE_FIXTURE=NativeFixture`; the protocol owner restricts this
identity to explicit loopback fixtures. Select an actual saved Java row in the
launcher. Production account routing still requires this fork's OAuth configuration.
The app waits for a nonempty spawn-section GPU upload before sending player-loaded;
Readiness alone does not establish gameplay support. Java loading readiness does not wait
for a Bedrock stream or send Bedrock loading commands. Empty section publications
use the shared zero-byte allowance. Native movement owns player translation; free
camera translation is disabled. Developer state exposes session/epoch, upload counts, worker
backlog, completion/obsolete/missing-light counts and explicit incomplete gameplay.

Live rendered evidence on Linux, 2026-10-22: headless software Vulkan (lavapipe),
1280×720 at scale 1, actual saved-server selection, offline vanilla login/configuration/
play, 49 columns and three real keepalives. Fresh inspected frames show grass,
stone, dirt and an oriented oak stair using the pinned runtime assets. A server
console dirt removal appears as the matching wall hole through incremental updates.
Save & Quit clears the native task/presentation records; the server reports zero
players. A same-client reconnect advances generation 2 to 4, renders another 49
columns and observes three more actual keepalives. Its fresh frame omits a stone
pillar removed while disconnected and retains the server-authoritative wall hole.
This tests server-driven rendering, not client breaking/placement or physical
movement. No Windows/macOS, visual parity or performance qualification is claimed.

## Native inventory authority

The target pins the item registry and maintained component/container schemas. Wire
item stacks preserve identity, count, component overrides and explicit removals;
missing overrides are not interpreted as missing default components. Common scalar,
text/NBT, enchantment, holder-set, tool, food and tooltip patches decode. Other
component payloads fail with their identifier; they are never skipped into a corrupt
snapshot. The expanded slot array has an explicit provisional 8 MiB budget, sharing
the transport inflation budget; this is not modded-container capacity qualification.
Complete content, slot, selected-slot, player-storage, cursor and menu
notifications enter `inventory::java` through `client-world`. Unknown slots remain
distinct from authoritative empty stacks. Player menus and supported chest layouts
reference one player-storage owner. Ordered state IDs may wrap and are not sorted.
Session/dimension replacement retires storage, menus and cursors; container IDs also
have local incarnations. Unsupported menu layouts expose an explicit inventory error.

Independent wire and encrypted-session tests, slot ownership, atomic invalid content,
state-ID wrapping and stale-session admission have deterministic coverage. Linux
hidden Vulkan acceptance (1280×720, scale 1, lavapipe) joins the offline vanilla
fixture through the launcher and admits 41 confirmed player-storage cells. The initial
sync exposed a retained carrot, correcting the earlier empty-hand assumption. Server
queries agree with an iron pickaxe carrying damage 7 and a count-7 dirt stack with an
explicit enchantments-component removal. A held item blocks the current empty-hand
breaking path. After an explicit server clear, admitted empty-hand actions remove dirt
and retain a rejected command block in fresh rendered frames. Fifteen keepalives are
observed before reconnect. Session 4→6 receives a fresh inventory and clears interaction
intent. Death clears the stack; the real Respawn button replaces inventory epoch 1→2
and receives a fresh empty menu with health 20. Fourteen keepalives are observed in
that reconnected session before shutdown. This is not inventory gameplay: native item assets,
click prediction/reconciliation, crafting, use-item and tool semantics remain incomplete. Component defaults are not yet resolved for behavior.
Chest layouts have deterministic coverage only, with no chest UI/click acceptance.
An incremental command-block addition during this run reproduced the existing
unprepared-state graphics failure. The subsequent interaction checks used states
present at join. The subsequent streaming extension and live acceptance described
above repair that failure without changing the inventory authority slice.
Focused owner/wire tests, app-inclusive clippy, formatting, architecture and the
Linux graphical build pass. An initial owner build exhausted disk; inactive artifacts
were removed and the identical checks passed. Online, Windows/macOS, full-workspace
merge gates and performance budgets remain unqualified.

Local number-key, wheel and controller selection uses the existing semantic input
router. The Java action owner retains desktop selection/attack order across bounded
transport backpressure. Selection serializes `ServerboundSetCarriedItemPacket` with
the pinned signed-short field; an explicit local write receipt enters the same ordered
admission queue as server selection facts. This receipt is not server acknowledgement.
No inventory stack is predicted or decremented. Later interactions wait for the receipt,
and server corrections can replace the selection before the next action. Buffered clicks
retain their original ray target. Input/session/
dimension/correction changes cancel unsent intent without replaying a consumed frame.
Complete click bursts remain ordered while selection waits for its local write receipt;
an incomplete physical input batch quarantines controls until release.
The existing HUD reads the native selected slot without
mutating Bedrock inventory. Native item icons and held-item models remain unsupported.
References are the pinned codec's `ServerboundSetCarriedItemPacket`, vanilla
`MultiPlayerGameMode.ensureHasSentCarriedItem` and
`ServerGamePacketListenerImpl.handleSetCarriedItem` from the verified runtime.

Linux hidden Vulkan acceptance (1280×720, scale 1, lavapipe) confirms number keys,
both wheel directions and wraparound against server queries and fresh HUD frames.
An occupied-hand attack followed by an empty-hand choice leaves dirt intact; reversing
those input edges sends the choice first and removes the block through a server update.
The inspected frames show both outcomes. Session 2→4 clears input/receipts and renders
fresh terrain and inventory; 22 keepalives precede reconnect and three follow it.
Save & Quit clears native state and the fixture is restored. Focused owner, encrypted-wire
and HUD regression tests, app-inclusive clippy, formatting, architecture and the Linux
graphical build pass. A dimension-replacement regression failed before preserving the
consumed input-frame identity and passes afterward. Live controller/touch input, online
mode, other platforms, full-workspace merge gates and performance remain unqualified.

## Native collision and dry-land movement

Native outline picking, server-authoritative creative breaking and main-hand block use
are implemented. Prepare the separate,
hash-pinned outline/interaction report with `assetc java-interaction --server SERVER_JAR
--out JAVA_ASSETS/interaction.bin --accept-eula`, using the same explicit Java/compiler
options as the physics command below. It runs only the original exporter against the
verified runtime. Contextual shapes remain explicitly unsupported; collision boxes
and visual meshes never substitute for outline shapes. The interaction adapter sends
sequenced native actions and retains backpressured intent, with input/session/dimension/
correction fences. Processing acknowledgements do not imply acceptance: only server
block updates change terrain. Developer state exposes selection, buffered/queued
intent and acknowledgements. Pending mesh dispatch prioritizes sections near the admitted
player eye without dropping distant work or bypassing publication lifetimes. Incremental
block/light changes now use spatial invalidation as described above; frame and edit-latency
budgets remain unqualified. Survival/tool
breaking, general item use, entity targeting, local hand/crack effects and inventory-based
restrictions are incomplete; this is not a playable-client claim.

Use input joins the same ordered queue as hotbar selection and attack. Block use sends
the pinned `ServerboundUseItemOnPacket` with main hand, signed packed position, native
face, fractional hit, inside/world-border flags and the shared action sequence. The
block picker produces block hits, not world-border hits. Socket dispatch checks loaded
dimension, correction and death fences. Held use repeats after the native four-tick
delay; a backpressured tap retains its original target. Terrain and inventory change
only through admitted server updates, so processing receipts never create a block or
consume an item. Unknown inventory, spectator mode and an opened unsupported container
block this path explicitly. Offhand fallback, air use, component-default behavior,
client effects, item visuals and container UI are incomplete. References are the pinned
codec's use-on-block schema/hand enum and verified vanilla `Minecraft.startUseItem`,
`MultiPlayerGameMode.useItemOn` and `ServerGamePacketListenerImpl.handleUseItemOn`.
To exercise it, join a saved Java server with the runtime assets configured above,
select a server-supplied block stack using number keys or the wheel, aim at a native
block outline and press/hold the existing Use binding (right mouse button by default).
Inspect `native_java.inventory`, `native_java_interaction` and `native_java.block_ack`
through developer control; item icons and held models do not yet show native stacks.

Linux hidden Vulkan/lavapipe acceptance at 1280×720, scale 1, enters the authorized
offline vanilla world from the launcher. Use on dirt places its neighbour and consumes
one item (7→6); an Adventure-mode rejection retains six and creates no ghost block.
An ordered slot-choice/use gesture places an oak stair (2→1). Server queries and fresh
frames agree on its south-facing, bottom, straight, dry state; its graphics extend
after join through the normal native mesh path. Reconnect advances generation 2→4,
restores authoritative terrain/stacks, and a new sequence-1 placement consumes the
last stair. Held use on that stair's top face places one dirt block (6→5), then rejects
further attempts into occupied space. A full-block fixture separately rejects repeated
placement into the player's collision volume without consuming items. Releasing Use
stops new requests while keepalives continue. Nine actual keepalives precede reconnect
and 30 follow it; Save & Quit clears native actions/world facts and the client exits.
The fixture is restored afterward. This is a limited main-hand placement slice,
not general item, inventory, combat or playable-client acceptance.

The use codec has independent signed-coordinate, hit/flag and malformed-value vectors.
The encrypted/compressed session fixture checks the official packet identity, field
order and retired correction/dimension fences. Gameplay tests cover slot/use ordering,
retained targets, no predicted stack consumption and a failing-then-passing held-use
tick-remainder regression. All 62 protocol unit tests, three session fixtures and
18 focused Java gameplay tests pass, alongside scoped app-inclusive clippy, formatting,
architecture and the Linux developer build. A low-disk linker failure recovered after
clearing inactive caches and rerunning the same build. Full-workspace merge gates,
Windows/macOS, online mode and performance budgets remain unqualified.

Linux hidden Vulkan acceptance (1280×720, scale 1, lavapipe) uses the authorized
non-operator offline vanilla fixture. Launcher input selects dirt by its native outline;
Attack sends the native action, the server confirms air, and a fresh frame shows its
removal. The same input against a command block receives a processing receipt and
unchanged-block correction; the block remains visible. After eight real keepalives,
Save & Quit clears selection, queued intent, receipts, meshes and entity poses.
Same-process reconnect advances generation 2→4, shows the server's replacement dirt
instead of the old command block, and starts with no interaction receipts or queued
actions. A new sequence-1 attack removes that replacement through a server update
and the renderer; eight more keepalives are observed before shutdown.

Independent shape/coordinate/packet vectors, malformed reports, backpressure,
input/correction lifetimes, encrypted stream ordering and mesh-priority regressions
pass at their owners. `assetc java-interaction` reproduces the manifest hash from the
pinned server runtime, and its input/output alias test passes. Initial live captures
exposed delayed mesh publication; the nearest-section regression fails before the
priority fix and passes afterward. No performance improvement is claimed: broad
column invalidation remains, and the live software-renderer run overlaps compilation.
Scoped app-inclusive clippy, formatting, architecture and the graphical build pass.
Full-workspace merge gates, Windows/macOS and online-mode acceptance did not run.

`assetc java-physics` verifies the manifest-pinned vanilla server archive and each
bundled library, compiles an original offline exporter and loads the resulting
report through `assets::registry::java::physics`. Only the generated report is a
client runtime input; neither the server classes nor a JVM run inside the client.
The report's byte length and SHA-256 live in `data/target.json`. The exporter also
verifies the 36 player-builder attribute bases against the generated
`data/player-attributes.json` manifest pin; those numeric facts initialize native
player authority before the server sends its changed attributes. It distinguishes
static double-precision boxes, dynamic shapes, non-air empty shapes, fluids,
friction/speed/jump/bounce properties and blocks with special movement callbacks.
Dynamic shapes never become empty collision. The authorized Linux fixture test
checks grass, air, stairs, water, ice, scaffolding and slime against the actual
exported report, independently of visual models and Bedrock registry IDs.

With a JDK matching the target manifest and an already obtained official server:

```sh
cargo run --locked -p asset-compiler --bin assetc -- java-physics \
  --server /path/to/server.jar --out /path/to/java-assets/physics.bin \
  --java /path/to/jdk/bin/java --javac /path/to/jdk/bin/javac --accept-eula
```

Only pass `--accept-eula` after accepting the Minecraft EULA. A locally supplied
ECJ compiler can replace `--javac` with `--compiler-jar`; the fixture used
`org.eclipse.jdt:ecj:3.44.0`, SHA-256
`14c46ddc2e511c46c998da728f6a80f13a689c920f3832402f7ad150f6a6188d`.
ECJ is an external preparation tool, under EPL-2.0; it is not a client dependency
or bundled asset. JDK/compiler discovery and acquisition are explicit caller inputs.

`sim::java::collision` contains separate double-precision clipping and stepping.
It rejects retired scopes, missing chunks, unknown states and dynamic shapes;
an explicit 32,768-cell foreground query budget remains a provisional limit.
Deterministic collision tests include independently evaluated vanilla diagonal
vectors. `sim::java::travel` applies dry-land acceleration, gravity, drag, jumping,
standing/crouched dimensions, collision and sneak-edge protection. Tests cover
falling, jumping, wall contact, forced crouching, missing terrain and atomic failure;
acceleration vectors were obtained independently from the pinned vanilla server.

`gameplay::java` advances the existing world-owned motion at 20 ticks/second and
retains one outbound frame under backpressure. Corrections retire old prediction
and force fresh pose/input baselines. Before sampling controls, a new correction
replaces stale camera angles even if its acknowledgement has already queued;
world publication runs after network send. The native socket checks session-owned
dimension and correction receipts before sending input, sprint changes, position/
rotation variants and tick-end packets. The application binds these to existing
semantic input and camera presentation; Java dimension epochs release held input.
Runtime loading now requires `physics.bin` alongside `client.jar` and `blocks.json`.
Developer state exposes `native_java.motion` and `native_java_movement` diagnostics.
Linux headless offline-fixture evidence includes walking across chunk boundaries,
sprint-jumping, crouching, exact wall contact and climbing the two halves of an oak
stair. Server entity data matched the stopped wall position
`[3.5, -60, 14.699999988079071]` and stair position
`[6.699999988079071, -59, 11.5]`. A mixed-action run answered 18 real keepalives
with 49 resident columns, no reported decode error and a drained movement/mesh backlog.
It exposed an angle-correction/camera ordering race. The regression fix passed a
fresh graphical run with absolute and relative angle corrections while walking and
sprinting: the server confirmed `0/45.000004` and `90/25.000004` degrees. That run
answered six real keepalives before Save & Quit cleared motion, the pending frame,
tick counters, meshes and the connection; the server reported zero players.
Same-process reconnect advanced generation 2 to 4, rendered 49 columns without
the test wall removed while disconnected, and answered four more keepalives.
New input moved the player to `[0.5, -60, 14.857479884892918]`, matching server data
exactly after stopping. Fresh inspected screenshots cover the corrected view,
pause menu, disconnect and reconnect on Linux at 1280×720, scale 1.
The server also resent support
blocks once at a jump landing; this is retained in acceptance evidence rather than
treated as an unexplained protocol success. The software-rendered process reached
about 1.8 GiB RSS; this is an observation, not a performance or memory qualification.
Flying, fluids,
special block callbacks, effects, entity collision and full respawn/dimension
movement retention remain incomplete; unsupported queries report a blocked movement
reason, never empty collision. These changes do not establish a playable client.

## Native entity asset preparation

`assetc java-entities` uses original export code with the verified server libraries
and client model factories. It writes local `models.json`, with its size/hash pinned
in `data/target.json`, and checks the generated entity schema report. The report
also exports the native default-skin table and player clothing masks; skin selection
uses Java UUID hashing and floor-modulo with independent pinned-runtime vectors. No official
client process or second renderer runs. The exported pig, cold-pig, baby-pig and
wide/slim player hierarchies retain pixel coordinates, face normals, winding, UVs,
parent transforms and visibility. `assets::entity::java` verifies and loads these
hierarchies. Runtime geometry and textures must remain outside source control.

After accepting the Minecraft EULA and obtaining the pinned archives:

```sh
cargo run --locked -p asset-compiler --bin assetc -- java-entities \
  --server /path/to/server.jar --client /path/to/client.jar \
  --out /path/to/java-assets/models.json --accept-eula
```

Use Java 25 with `javac`, or supply `--java /path/to/java` and
`--compiler-jar /path/to/ecj.jar` as described for collision preparation. Linux
fixture tests reproduced both entity report hashes and the existing collision and
player-attribute hashes after the exporter infrastructure was shared. Loader tests
reject incorrect provenance, duplicate parts and invalid normals. These checks
establish prepared model provenance; rendered acceptance is described below.

## Entity lifecycle work in progress

The target manifest also pins 161 native entity identities, checked against the
official registry report. Lifecycle schemas cover spawn, linear/stepped relative
motion, position paths, rotation/head rotation, teleport and removal. Native world
admission separates network IDs from session/dimension/incarnation identities,
retains the exact delta anchor and preserves teleport receipts across coalesced
rotation updates. Independent vanilla delta vectors cover negative rounding ties
and zero-axis precision; tests cover atomic failure, ID reuse and retired scopes.
The encrypted/compressed session fixture checks ordered spawn/move/remove delivery.
Scoped protocol/domain tests, application-inclusive clippy with tests and the
architecture gate pass on Linux; full workspace/merge and other-platform gates
have not run for this slice. Subsequent metadata tests passed 52 protocol unit tests,
three encrypted/session fixtures, 17 client-world Java tests and 14 world Java tests.

A Linux offline vanilla diagnostic run admitted 31 spawns, 990 movement packets
(768 nonzero), 56 position paths and 12 removal packets, with three real keepalives.
A tagged pig spawned at `[3.5,-60,8.5]`, received a server position path ending at
`[5.5,-60,9.5]`, then disappeared through its removal event. This verifies wire and
domain admission only. Metadata serializer ordering and native pig/player/item
accessor schemas now come from pinned references and the original runtime exporter.
Pig appearance resolves through received configuration registries, including the
runtime-derived default variant. A later Linux offline run changed a tagged pig
from temperate/adult to cold/baby/glowing through server commands; admitted facts
matched variant, texture identifiers, age flag, glow flag and health while three
real keepalives and 49 chunks were observed. The encrypted fixture also covers
metadata delivery. Local metadata follows respawn retention without creating a
second local-player entity. Invalid updates are atomic and stale epochs are rejected.

Complex item components, nonempty particle metadata, embedded profile metadata and inline painting
holders remain unimplemented. Their known passive metadata notification produces an
explicit, persistent incomplete-entity diagnostic; no parsed prefix is admitted.
Other entity accessor schemas remain incomplete. Correctly framed unused accessor
values (including names/equipment-related metadata) do not imply implemented visuals.
Native pig and default-skin player bodies publish through Cinnabar's existing actor renderer. An off-frame,
cancellable worker validates `models.json` and the pinned client archive, compiles
native hierarchies into ordinary GPU bone transforms and prepares registry-selected
textures. The actor catalog uses a separate Java GPU namespace, not Bedrock entity
IDs. Session/dimension/incarnation fences retire cached poses and pending workers;
only the active edition prepares the shared actor scene. Model changes replace the
pose and culling bounds; removed entities leave the next complete frame batch.

Player-list update/removal schemas follow the manifest-pinned MCProtocolLib
`ClientboundPlayerInfoUpdatePacket` and `ClientboundPlayerInfoRemovePacket`; action
bits generate from the hash-pinned `PlayerListEntryAction` enum. `world::java::roster`
retains profiles separately from entity IDs. Native two-pass admission keeps the first
profile on duplicate additions, preserves omitted fields, and applies explicit clears.
Unknown updates are counted without inventing players. Profiles survive dimension
replacement and retire on session/configuration replacement. Remote entities cache
their immutable profile until despawn, so removing a tab entry does not erase a body.
The developer state exposes roster counts and sampled public identities, not properties.

Remote chat key encoding is checked using the existing RSA library. Received public
keys/signatures remain explicitly **unverified**; no chat trust or signing capability
is granted. Signed chat and custom texture signature verification/downloads are not
implemented. Default-skin fallback is reported through `custom_skin_pending` when a
profile supplies textures. It does not establish custom-skin fidelity.

The player body uses the exported wide/slim hierarchy, native scale, metadata clothing
visibility, independent player body turning, and standing empty-hand gait/idle poses.
It reuses the existing actor rig and generic ordered interpolation track, not pig body
control or Bedrock movement. Unarmed body/model probes and lifecycle/visibility tests
pass; full math equivalence, item-use, attack, crouching, mounted, swimming and spectator
presentation remain unqualified. The tab-list UI, nametags, equipment and capes remain
open. This is not a playable-client claim.

Two native Cinnabar clients joined the pinned offline vanilla fixture from their
saved Java launcher entries on Linux (hidden Vulkan/lavapipe, 1280×720, scale 1).
Fresh frames show the second player's default skin, correct standing hierarchy and
moving limbs alongside actual terrain and the pig. Native input moved the peer;
the server position and observer's quantized position agree within one wire unit.
Departure removes the roster entry, body and cached pose. Rejoin creates one new
entity incarnation; observer reconnect advances generation 2 to 4, clears old
terrain/poses, and renders one peer and one pig. Eight keepalives were observed
before observer reconnect and two afterward, with no unknown roster/entity updates
or entity asset errors. These fixtures qualify neither custom skins/equipment nor
general movement, visual or performance parity.

The local evidence is `java-player-{joined,walking,departed,rejoined,main-reconnected}.png`,
`java-player-walk.mp4`, and corresponding state/build records outside the checkout.
The tested binary SHA-256 is
`4fe80b76c8bc4574bf1811bf508b47b9e49230364f86b888f049e08703071f48`;
it includes this slice over `3cac1cb77`, with only Rust formatting changed afterward.
Owner tests cover wire actions and malformed input, default skins, profile lifetime,
entity-ID reuse, body/gait vectors and clothing visibility. Scoped tests, app-inclusive
clippy, graphical build and the architecture check pass. The first link exhausted
disk space; clearing idle build caches allowed the unchanged build to pass. Windows,
macOS, online authentication and full integration/merge gates were not run.

To exercise this slice, place the exported `models.json` beside `client.jar`,
`blocks.json` and `physics.bin` in `CINNABAR_JAVA_ASSETS`, build the client as above,
and select the saved Java fixture entry. Developer state exposes
`native_java_entities`: published bodies, unsupported entities, missing light,
pending glow outlines and preparation errors. Missing or invalid resources fail
explicitly rather than substituting unrelated artwork.

Linux headless lavapipe frames at 1280×720, scale 1, show the authorized vanilla
fixture's temperate adult and cold baby pigs on actual server terrain. Server
rotation, a baby-to-adult change and removal changed the rendered bodies correctly.
The final build also moved a pig through server updates, then Save & Quit cleared
all cached poses. Same-process reconnect advanced generation from 2 to 4 and rendered
one surviving pig without the removed body. Both sessions answered two real
keepalives with 49 resident columns. Actor regression tests passed 155 cases; five
existing external-fixture/benchmark tests were skipped. Scoped application-inclusive
clippy with tests, formatting, architecture and the developer client build passed.
Full workspace/merge gates, Windows and macOS have not run for this slice.

Native pig presentation retains ordered position paths and interpolates them on a
20 Hz timeline independently of authoritative positions and delta-decoding anchors.
The domain owner supplies wrapped body/head rotation and quadruped walking poses;
the application samples those poses for the existing actor renderer. Independent
vectors from the pinned runtime cover stepped turns, head/body control and walking
phases. Admission tests cover coalesced updates, stale epochs and metadata/velocity
updates that must not restart interpolation. A provisional global budget of 262,144
pending steps fails explicitly rather than dropping movement; larger workloads are
unqualified. At most four ticks run per rendered frame, retaining excess elapsed
time and exposing the backlog in diagnostics.

Linux headless lavapipe acceptance at 1280×720, scale 1, records a real vanilla
AI walk toward the fixture player. Successive rendered frames show alternating
leg poses. In-flight diagnostics distinguish interpolated positions from wire
targets; the queue drains and the stopped position matches the server within
wire quantization. Removal clears the body and pose cache. Save & Quit clears
the clock, scope and terrain work; reconnect advances generation 2→4 and renders
the replacement pig with a fresh gait. The first session answered 14 keepalives
and the reconnect answered two. Eleven domain and six admission tests, scoped
application-inclusive clippy with tests, formatting, architecture and the developer
build pass. This run does not qualify frame budgets or Windows/macOS behavior.
The developer-control `in_world` wait now requires a position and uploaded terrain
from the current Java play session and dimension. Loading, reconfiguration, failed
uploads and retired scopes cannot satisfy it. The normal Bedrock path is retained.
This diagnostic identifies world entry, not complete gameplay support.
Linux fixture verification confirms the wait succeeds after a real launcher join
and world entry clears after Save & Quit. The run answered 26 keepalives.

Native teleport and absolute position-sync semantics remain separate: teleport
preserves the delta-packet anchor and resolves relative fields against an active
target or the completed pose. Short teleports interpolate; position-sync rebases
the codec and snaps changes beyond the native distance cutoff. Completed rotation
samples normalize angles independently of the raw target. Independent pinned-runtime
probes and follow-up wire deltas cover these distinctions; regressions first exposed
the wrong anchor and a distant sync retaining an obsolete visual target. Long
teleport reconciliation with collisions and non-ticking entities remains unqualified.
Linux hidden Vulkan acceptance now renders an 80-block vertical position-sync
onto a server-created grass platform with the same pig visible. Native unallocated
light lookup fixes the earlier missing terrain/body capture. Resident nonempty
sections advance 25→27 and return to 25 after server removal; actor missing-light
count stays zero. Server entity data confirms movement on that platform. The run
observed seven keepalives before Save & Quit cleared meshes, poses and clocks;
reconnect advances session 2→4 and renders fresh ground terrain and one pig.
This is an offline vanilla fixture at 1280×720, scale 1, software lavapipe.
Server-console block edits do not establish client interaction support. The six
Java owner/consumer suites pass 69 tests, including a regression that failed before
the fix, plus app-inclusive clippy with tests, format, architecture and a graphical
build. No-sky dimensions have deterministic coverage only; smooth lighting, local
light propagation, frame budgets and Windows/macOS remain unqualified.

This remains a narrow native entity slice. Tick-rate changes/freezing, shadows,
glow outlines, death animation,
custom player skins, equipment and other entity models remain unsupported.
Lighting currently samples the authoritative feet cell. These limitations remain
visible in diagnostics and do not establish entity visual parity or a playable client.
Passenger relationships retain scoped lifetimes with deterministic replacement/cycle
coverage, but mounted motion and seat presentation are unsupported.

## Native status HUD and respawn

The target manifest now pins the maintained game-mode and game-event enums. Native
experience and game-mode notifications decode in the protocol owner. `client-world`
projects admitted health/food, attribute-derived maximum health/armor, experience
and mode into `world::java::hud::Snapshot`; UI rendering consumes that domain
snapshot rather than synthetic Bedrock attributes or packet-library types.
Session/dimension checks reject stale projections and replacement players clear
unknown vital/experience values. The existing respawn intent now selects the native
command queue for Java. Status-row and hand visibility use the native mode explicitly.
HUD capture preserves Java's synchronized armor instead of replacing it with the
Bedrock inventory-derived value. The unimplemented native attack-charge indicator
stays absent.

The authorized Linux offline vanilla fixture was exercised through the launcher and
existing JSON-UI renderer at 1280×720, scale 1, with software Vulkan. Server queries,
MCP state and fresh frames agree on health 13, food 18, armor 6, level 7 and experience
progress 0.47619048. Removing the chestplate removes the armor row. Mode changes hide
survival stats, and spectator mode hides the hand. A server kill opens the real death
screen; its Respawn button returns health/food 20 and level 0, replaces dimension
epoch 1 with 2, and reloads terrain. Save & Quit clears the native HUD projection,
terrain work and entity poses. Same-process reconnect uses a fresh session generation.
This run is offline-mode evidence only, not online or cross-platform acceptance.

Focused tests cover independent wire vectors, encrypted/compressed event ordering,
stale HUD lifetimes, malformed values and death state. The capture regression first
reproduced the armor overwrite, then passed alongside existing Bedrock armor tests.
Scoped clippy, architecture and formatting checks pass. The graphical link initially
failed at the workspace disk limit; after removing inactive caches, relinking passed.
No full-workspace merge gate or hardware frame-budget qualification ran for this slice.

This is numeric status through the existing presentation, not Java HUD visual parity.
Absorption, effects, air,
Java damage flashes, signed chat, immediate-respawn rule changes and inventory
presentation remain incomplete. The pinned client's `Hud.extractPlayerHealth`,
`LivingEntity.getArmorValue` and `FoodData` were consulted for ceil health, floor
armor and food display units; no Mojang source or art is redistributed.

## Dependency choice and notices

The starter's 15 tests covered status only and were rerun before extension.
Azalea at `0d38e0dcf47eb258c41b1b09ebc2bcde7253e185` was evaluated, but its
protocol target lagged the selected release and required a nightly toolchain and
broader dependencies. Importing its engine was unnecessary. MCProtocolLib at the
manifest revision provides the matching codec reference; it is not a runtime
Java client. No second renderer or game engine is imported.

RustCrypto `aes`, `cfb8`, `rsa`, `sha1`, `md-5` and `rand` supply the cryptographic
primitives; protocol hashes use SHA-1/MD5 because Java specifies them. Production
RSA only encrypts the server challenge; it never decrypts private-key operations.
`flate2` uses its Rust backend, `reqwest` uses rustls, and Hickory supplies DNS.
These are stable-Rust libraries, with exact revisions/checksums in `Cargo.lock`.
Relative to the fork base, the lockfile adds 32 external package/version pairs
(and the two local crates), including DNS, caching and crypto support. Existing
HTTP/TLS packages are reused. No renderer is added. The transport/auth futures
are cancelled by dropping their owning task; queues apply backpressure.
MIT/Apache-2.0 (and the selected alternative licenses recorded by Cargo metadata)
notices must be preserved by packaging; see the repository third-party notices
for the pinned codec reference. The MIT codec reference retains its upstream license. No Mojang code or server
assets are redistributed.
