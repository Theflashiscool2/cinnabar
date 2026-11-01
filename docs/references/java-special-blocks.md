# Native Java special block presentation

Scope: Java 26.3 / protocol 777, using the verified archives pinned in
`crates/java-protocol/data/target.json`. Ordinary baked models, invisible blocks,
fluids and native special submissions have distinct admission paths. Unsupported special
renderers still produce grouped audit errors.

## Chests

`assets::registry::java::special` owns native chest identities, facing, pairing and
texture selection. It does not reinterpret Bedrock block IDs. The pinned registry's
contiguous state groups reproduce the vanilla static block registry order, checked
against `BuiltInRegistries.BLOCK` (1,286 blocks / 35,723 states).

Normal, trapped, ender and the four copper oxidation stages (including waxing)
use Java textures and separate double-chest halves. Geometry, UV layout, orientation,
lid pivot and easing were checked against ChestModel, ChestRenderer and ModelPart.Cube
in the verified client. The emitter is original code; Java and Bedrock cube unwraps
differ. Missing/incompatible neighbors temporarily use a single chest and recover
when authoritative neighbors arrive. Waterlogged chests retain native fluid geometry.

Block events supply viewer counts; the local inventory screen does not set openness.
Incremental block-entity data is admitted through the session's terrain generation.
Same-block property updates preserve entity data; replacement, unload and session
changes retire animation state. GPU publication compares retained vertex snapshots
as well as local revisions, which can restart when a scene is replaced.

Linux Xvfb/lavapipe controlled offline vanilla testing inspected single/double geometry,
four orientations, trapped/ender/copper textures, waterlogging, server-driven opening
and closing, unpairing, removal, a negative chunk boundary, unload/revisit and reconnect.
Opening used native mouse input. Closing was also exercised through a server game-mode
change; this is not evidence of native container UI. The server's 27/54-slot inventory
data arrives, but the native Java container screen and outbound close/click UI path
remain unintegrated in this checkout. Replacing menus without a close can leave server
opener counts elevated. Do not advertise functional inventory transactions from these
presentation tests.

Seasonal textures, chest sounds/particles and user resource-pack reload are not qualified.
The existing fluid-height quantization limitation also applies to waterlogged hosts.
No iOS, simulator, online-mode or Hypixel acceptance follows from these Linux results.

## Shulker boxes

All 17 native texture identities and six attachment directions use Java ShulkerModel
shell geometry, Direction rotation (including roll), and ShulkerBoxRenderer lid travel
and rotation. The pinned textures contain deliberate cutouts; the reference uses
`RenderTypes.entityCutout`. They are preserved. Native cube UV coordinates were checked
independently against ModelPart.Cube, not inferred from Bedrock unwraps.

Viewer-count events drive a 20 Hz simulation. Zero closes, one opens, and larger counts
retain the previous direction. Coalesced events preserve the last zero/one transition.
Picking reads the tick's expanded bounds; rendering interpolates the shell. Motion survives
same-block facing admission and resets with the authoritative block lifetime. Unresolved
contextual shapes still fail safely; they do not become empty selection geometry.

Linux software-Vulkan offline vanilla acceptance inspected six directions, native mouse
opening, another real Cinnabar client's observation of opening/closing, negative-boundary
side opening, facing updates, atlas extension, removal/replacement, unload/revisit and
reconnect. Spectator mode did not close the server shulker container; that attempt is not
passing closure evidence. Closure was observed after the creative viewer disconnected.
The Java inventory model still rejects SHULKER_BOX and has no usable container screen.
These tests establish presentation and interaction packet delivery, not item transactions.
Dynamic physical pushing, sound, and end-user resource-pack reload remain unqualified.

## Banners

Native banner dispatch uses Java BannerModel/BannerFlagModel geometry and the 26.3
BannerRenderer mount transforms. Standing rotation has 16 segments; wall mounts have
four directions. Cloth uses the reference position-dependent 100-tick wave and pivot.
Its current phase uses the scene clock; synchronization to the server's new world-clock
packet is not yet implemented. This is not a claim of full atmosphere/time parity.

Pattern registry keys and asset IDs come from the pinned archive's 43 data definitions,
not Bedrock short codes or guessed filenames. Base dye colors were independently checked
against the verified runtime's DyeColor.getTextureDiffuseColor. The first 16 authoritative
NBT pattern layers render in order, with matching coplanar geometry, alpha blending and
shared actor lighting. Missing block-entity data gives the reference plain banner; malformed
or unsupported pattern content produces an explicit diagnostic. Arbitrary custom pattern
registries/artwork and end-user pack reload remain outside this admitted default-asset path.

Changed-column extraction caches parsed patterns. Data updates, replacement, resource
identity changes and world retirement invalidate that cache. The special atlas loads on
workers and is published only against its requested resource identity. No Bedrock renderer
or numeric dye mapping is changed.

Automated coverage includes all 320 states and verified pattern textures, geometry for
all standing/wall orientations, layer order/bounds, negative positions and malformed data.
Linux software-Vulkan offline vanilla acceptance inspected the mount gallery, layered colors,
authoritative pattern updates, native creative breaking with server confirmation, position reuse
with a different banner, a negative chunk boundary, unload/revisit and reconnect. The prior
ordinary-model join gate was reproduced before repair. User resource-pack reload and
server-synchronized cloth phase remain unqualified; these are not iOS or Hypixel results.

## Fixed mob heads

Skeleton, wither skeleton, zombie, creeper, dragon and piglin heads use native Java
state admission, archive textures and a separate special-mesh submission. Ground mounts
have 16 rotations; wall mounts have four directions. Zombie hats retain the reference
inflation and texture coordinates. Dragon jaws and piglin ears use bounded 20 Hz powered
animation, preserving the stopped pose until power resumes. Block generations retire the
clock on replacement, unload and world/session changes.

Original emitter code was checked against SkullBlockRenderer, SkullModel, PiglinHeadModel,
DragonHeadModel and ModelPart in the verified target archive. The independent probe under
`tools/java-reference` compares all positions and UVs for four models, three rotations and
three animation phases (36 cases), including mirrored dragon horns. A verified-asset fixture
covers all 240 fixed-head states and six textures. These CPU checks are not visual acceptance. Linux software-Vulkan offline vanilla frames additionally inspected standing/wall mounts, native creative breaking, authoritative replacement, powered motion and frozen unpowered phases, a negative chunk boundary, unload/revisit and reconnect. The old skull join gate was reproduced before repair.
Player-profile heads remain explicit unsupported states; a fixed mob texture is never used
as a substitute for a player skin. End-user resource-pack reload is not yet qualified.

## Copper golem statues

Native statue admission covers four oxidation stages, waxing, four poses, four facings
and waterlogging. Geometry comes from the existing verified `ExportEntities` pipeline:
CopperGolemModel's standing/running/sitting/star factories are added to `models.json`.
The original five actor hierarchies remain unchanged. `assetc java-prepare` detects an
old report by its pinned hash and regenerates it; packaging retains the same six input files.
No proprietary model geometry is committed and no official game process is used at runtime.

The native special pass retains the exported hierarchy, face UVs and visibility. It applies
CopperGolemStatueModel's root overrides and the block renderer's facing transform; shared
actor lighting and resource publication remain the GPU owners. Water uses the existing native
fluid host facts. A missing model report fails explicitly rather than admitting empty geometry.

An independent Java probe compares positions, UVs and outward normals for all 16 pose/facing
combinations. Tests also cover negative-coordinate translation, 256 states, the four textures,
missing hierarchy rejection, exporter reproducibility and existing automatic preparation.
Linux software-Vulkan offline-vanilla captures inspect the oxidation/pose gallery, native
right-click pose cycle confirmed by server state, waterlogging at a negative chunk boundary,
and authoritative removal/replacement. Unload/revisit and reconnect are checked separately.
Arbitrary end-user pack reload, iOS and online-server acceptance remain unqualified.

## Remaining special renderers

Player-profile heads,
End gateway/portal and moving pistons still require native production integration.
Their audited states remain explicit failures. Storage alone is not presentation.

## Decorated pots

The native path uses the pinned `DecoratedPotRenderer.createBaseLayer/createSidesLayer`
export, native facing transform, texture UVs and named back/left/right/front item templates.
Pattern assets resolve through the archive registry. Default item-pattern correspondence is
checked against all native component initializers; explicit provides-pottery-pattern patches
and removal override the default. Missing block-entity data produces the native plain pot.
Malformed decoration structures and unavailable pattern holders remain explicit errors.

Server block-event action 1 starts the positive seven-tick or negative ten-tick wobble.
The bounded clock deduplicates event revisions, resets when a block generation disappears,
and retains its state across artwork changes. Geometry/data extraction uses changed columns;
it does not reparse all block entities each frame. Linux software-Vulkan offline-vanilla captures inspect four decoration faces, authoritative
pattern changes, removal/replacement, negative-boundary waterlogging, unload/revisit and
reconnect. Native survival use inserts bricks (server pot 0→2, stack 8→6); a full-pot
rejection preserves both counts. Real-time recordings inspect both wobble styles. The native
hotbar still lacks the held brick icon; this is not inventory UI acceptance. Arbitrary user
pack reload and iOS/online acceptance remain unqualified. The shared Bedrock pot path remains separate.

## Conduit

The native path exports `ConduitRenderer` shell/cage/wind/eye layers and uses the
Java PNG/mcmeta wind timeline (22 frames, three ticks each in the pinned archive).
It reuses the shared special atlas and cutout GPU pass, with native floating cage,
three wind orientations, inner wind and camera-facing open/closed eye transforms.
Exported face normals avoid precision loss on the thin eye edges. Existing Bedrock
conduit behavior is unchanged.

The client derives activation from 27 water-tagged cells and the server's
`minecraft:conduit_effect_block` tag at the native frame positions: 16 activate,
42 open the eye. Missing neighbors are temporarily inactive and recover on the
40-tick check. Block generation, dimension and session changes retire animation
state. The check cadence is local; synchronization to the server game-time modulo
40 remains unqualified, as do conduit particles/sounds and arbitrary resource packs.
Structural admission requires all four verified models and all artwork; it does
not make unsupported states invisible. Inspected Linux software-Vulkan offline-vanilla frames
and real-time recording cover inactive shell, 16-frame activation, 42-frame open eye,
41-frame closed eye, 15-frame deactivation, water obstruction/recovery, removal/replacement,
negative chunk boundaries, unload/revisit and reconnect. Server effect queries independently
confirm Conduit Power addition and expiry. No iOS, online or Hypixel acceptance is claimed.

## Native End portal

The pinned archive's `TheEndPortalBlockEntity.shouldRenderFace` exposes both Y faces;
`TheEndPortalRenderer.TRANSFORMATION` places them at 3/8 and 3/4 block height.
`RenderPipelines.END_PORTAL` selects 15 star layers, while the gateway selects 16.
The Java dispatch uses the archive's End sky and portal textures, and reads only the
bounded numeric palette from its shader resource. Resource shader code is never executed.
The shared atlas/GPU pass has an explicit Java screen-projection branch; Bedrock's
world-projected layers are unchanged. Geometry, atlas lifetime and publication reuse the
existing special-model owners. Local animation phase and shared fog are not claims of
full server-clock/fog parity. Gateway beam/lifecycle remains gated independently.

Behavior references: verified 26.3 client archive, `AbstractEndPortalRenderer`,
`TheEndPortalRenderer`, `TheEndPortalBlockEntity`, `RenderTypes`, `RenderPipelines`,
`GlobalSettingsUniform` and the portal shader's projection/layer behavior. Implementation
is original Rust/WGSL; copyrighted shader code and artwork are not distributed here.

Controlled portal entry exposed a distinct respawn admission failure: the new dimension
correctly retired its placed player body, but also discarded the rotation/velocity used by
vanilla's first relative correction. Native `ClientPacketListener.handleRespawn` retains
these with KEEP_ENTITY_DATA and otherwise starts at yaw -180, pitch zero and zero velocity.
Admission now keeps that correction basis separately from movement authority, consumes it
once, and still rejects relative positions without a placed anchor. Session/epoch checks
and teleport acknowledgements remain authoritative. Inspected Linux Xvfb/lavapipe vanilla
26.3 offline acceptance covers top/bottom surfaces, animation, removal/replacement,
negative chunk boundaries, unload/revisit and reconnect. Server-admin portal entry and
repeated Overworld/End transitions now retain admitted authority; reconnect in the End
passes. The first movement attempt used MCP `press` (one frame), not `hold`, and did not enter
the portal; it is not walk-through acceptance or evidence of a movement implementation defect. The captures also expose Overworld sky/clouds
in the End: atmosphere remains a separate implementation defect. User-pack reload,
iOS and Hypixel acceptance are not established.

## Native End gateway (integration under validation)

Gateway geometry uses a full six-face cube and sixteen projected layers, with native
neighbor full-face occlusion. Fluid-height coverage is deliberately a different report
field: the native empty gateway shape makes partial-neighbor rules differ. Schema 2 of
the verified fluid/host report retains every prior fact and adds the native full-face
identity mask; an exhaustive independent client probe checks all six directions.

The supplied gateway beam texture uses the native rotating 0.15-radius opaque inner
column and 0.175-radius, 32/255-alpha outer column. Age and authoritative cooldown events
control the native sine height/UV scale; BE receipt revisions distinguish repeated age
updates from unrelated column changes. Generation retirement prevents animation leaking
across position reuse or sessions. Clock phase is currently local; full native fog/time
parity remain pending. Linux software-Vulkan offline vanilla captures verify the cube,
spawn and authoritative cooldown beams, full/partial neighbor occlusion, removal and
replacement. Native held-input entry now receives the server teleport, including at a negative
chunk boundary. Unload/revisit and reconnect retire and rebuild gateway presentation.
These Linux controlled offline results do not qualify iOS/Hypixel.

Gateway contact is server-owned in 26.3: `EndGatewayBlock.entityInside` returns on
the client. The physics exporter excludes only that callback from unsupported local
travel flags; all native collision shapes and other callback gates are retained. The
server still decides eligibility, cooldown, destination and corrections. The corrected
report requires regeneration through `assetc java-physics`. The rebuilt client passed
held-input entry, authoritative teleport, negative-boundary entry, unload/revisit and
reconnect with inspected software-Vulkan captures. End portal contact remains gated
separately; arbitrary resource-pack reload and exact server-clock phase are unqualified.

## Player-profile heads (preparation, not admitted)

`java_profile` extracts 26.3 profile NBT, including list-shaped properties and compact
names, checked with the original `InspectHeadProfile` codec probe. Absent profiles
remain distinct from present profiles with nil UUIDs. Resource texture patches are
still explicit gaps. `skull::skin` prepares head texels using the native size and alpha
rules; legacy body limbs are not synthesized by this head-only helper.

The shell's Java skin resolver validates texture properties and official texture URLs,
upgrades HTTP, rejects redirects, bounds reads and supports cancellation. Its operational
property/download budgets are provisional limits, not full resource parity. A host-owned
bounded cache, per-instance skull texture selection and immutable atlas publication
are still required. All 40 player-head states remain gated; these helpers alone are
not player-head presentation or live-download acceptance.
