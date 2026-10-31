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

## Remaining special renderers

Banners, heads/skulls, copper golem statues, conduit, decorated pots,
End gateway/portal and moving pistons still require native production integration.
Their audited states remain explicit failures. Storage alone is not presentation.
