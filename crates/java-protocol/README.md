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
`Cargo.lock` pins Rust dependencies. Other Java versions are rejected by the
launcher and acceptance tool; a status response alone never establishes support.

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
the rendering thread; entity/inventory notifications remain outside admitted state;
the renderer receives no synthetic Bedrock packets or fabricated world.

Unsupported required resource packs, experimental features, transfer, consent
or interactive dialogs, an initial relative teleport without an admitted anchor,
mid-session reconfiguration and tag replacement fail
clearly. Entity/inventory decoding, movement, chat signing and interactions remain
unimplemented. Native assets/mesh publication have the limited offline rendered
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
new correction receipt to the camera. Physics and outbound movement remain separate,
unfinished work. Deterministic coverage checks encrypted wire ordering, cancellation,
timeouts, correction precision and stale receipts.

Player abilities, attributes and packed `LpVec3` velocity now have typed decoders and
local-player admission. The attribute ID table is generated from the hash-pinned
`AttributeType.java` reference; all 40 IDs were checked against the official registry
report whose hash is recorded in the target manifest. Generic attribute defaults
are not substituted for server-supplied player values. Remote entity dynamics remain
explicitly deferred until their lifecycle owner exists. Velocity before initial
position does not invent a spawn anchor. Respawn retains attribute bases and obeys
the server's modifier-retention flag; metadata retention is recorded but entity
metadata itself is not implemented.

Angle-only corrections use the same bounded, ordered admission deadline and receipt
fence as teleports. Their resolved rotation is returned in Java's rotation packet;
camera updates use a domain pose revision independent of teleport IDs. Independent
vanilla velocity vectors, malformed payloads, encrypted rotation-response ordering,
attribute arithmetic, admission atomicity and stale identities have deterministic
coverage. This does not establish client walking or general entity synchronization.
Linux offline vanilla diagnostics also admitted absolute rotation `90/25`, relative
`+15/-10`, and a return to `0/0`; server entity data confirmed `90/25` and `105/15`
after the replies. That run retained 49 columns, positive health and four real
keepalives before clean disconnect. Angle-only camera presentation is implemented
but awaits a fresh graphical run; this live evidence is from the diagnostic client.
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
terrain/light is unknown, not air or fullbright. Session/dimension epochs and
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
incomplete. Graphics buffers currently rebuild for a collected initial
state set; streaming new state identities needs an incremental resource extension.

`meshing::java` consumes immutable domain snapshots and produces shared Cinnabar
mesh buffers, sampling the server's separate sky/block light nibbles.
`chunk-pipeline::java` bounds concurrent jobs/results, carries neighbor revision
keys and rejects obsolete work. Unknown light remains pending until new authority
arrives. Its uniform-biome path preserves the actual native registry index and
fails explicitly when Java Voronoi/blend processing is required.

For CPU mesh acceptance, add `--client-assets CLIENT_JAR --compile-terrain` to the
command above. Linux offline vanilla acceptance compiled five real state identities
into seven materials, thirteen templates and 521 visible model quads across 24
center-column sections; the sampled oak stair produced eleven source model quads.
That run observed 49 columns and two real keepalives. `rendered_world` stayed false.

The graphical experimental path loads `client.jar`, `blocks.json` and `physics.bin` from
`CINNABAR_JAVA_ASSETS` (default `.local/assets/java/<pinned-version>`).
Build with `cargo build --locked -p cinnabar --features developer-control` and
launch through the repository's headless client MCP. Developer builds alone accept
`CINNABAR_JAVA_OFFLINE_FIXTURE=NativeFixture`; the protocol owner restricts this
identity to explicit loopback fixtures. Select an actual saved Java row in the
launcher. Production account routing still requires this fork's OAuth configuration.
The app waits for a nonempty spawn-section GPU upload before sending player-loaded;
this is not a claim of movement or gameplay. Java loading readiness does not wait
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

## Native collision and dry-land movement

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
