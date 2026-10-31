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

## Remaining special renderers

Player-profile heads, copper golem statues, conduit, decorated pots,
End gateway/portal and moving pistons still require native production integration.
Their audited states remain explicit failures. Storage alone is not presentation.
