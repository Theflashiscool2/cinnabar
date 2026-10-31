# Native Java special block presentation

Scope: Java 26.3 / protocol 777, using the verified archives pinned in
`crates/java-protocol/data/target.json`. Ordinary baked models, invisible blocks,
fluids and chest submissions have distinct admission paths. Unsupported special
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

## Remaining special renderers

Banners, shulker boxes, heads/skulls, copper golem statues, conduit, decorated pots,
End gateway/portal and moving pistons still require native production integration.
Their audited states remain explicit failures. Storage alone is not presentation.
