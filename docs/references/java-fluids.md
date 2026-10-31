# Native Java fluid presentation

Scope is the version in `crates/java-protocol/data/target.json`. The original
`ExportFluids.java` exports state/fluid facts from the hash-verified server;
`java-prepare` verifies and installs `fluids.json`. No JVM runs inside Cinnabar.
The iOS asset injector includes this sixth runtime input. Re-run `make java-assets
JAVA_ACCEPT_EULA=1` when upgrading from a five-input build.

Behavior references inspected in the verified 26.3 archive are `FluidRenderer`,
`FluidStateModelSet`, `FlowingFluid.getFlow`, `FluidState.getOwnHeight`,
`FluidState.shouldRenderBackwardUpFace`, `BlockState.getFaceOcclusionShape`, and
`Shapes.blockOccludes`. Reference implementation code is not redistributed.

Java palette IDs select independent fluid facts. Invisible fluid hosts produce
liquid quads; waterlogged model hosts retain both their model and liquid geometry.
Other empty special models still fail explicitly. Shared liquid buffers, texture
arrays, animation timelines and publication fences are reused. Java-specific flags
select half-tile flow UVs and exposed-bottom separation without changing Bedrock UVs.

Corner heights use the center, side and eligible diagonal samples, weighted by
source height. Same-fluid neighbors suppress internal faces. Host and neighbor face
occlusion use exported directional shape coverage; overlays apply to the reference
water-overlay neighbors. Missing neighbors temporarily expose surfaces and existing
column/light publication revisions invalidate them when data arrives or disappears.
Water uses biome tint and alpha; lava uses the depth-writing route and emitted light.

Limits remain explicit: corner heights use the shared 1/255-block representation
and horizontal flow uses signed 8-bit components. The exported fluid-flow-blocking
tag is the vanilla default; server tag overrides are not yet reflected in that
predicate. The current Java stream still rejects nonuniform biome neighborhoods,
and spatial biome grass modifiers remain gated. Resource-pack application and visual
parity are separate acceptance requirements; a successful model audit proves neither.

Focused tests use `CINNABAR_JAVA_TEST_ASSETS` for verified runtime data and
`cargo test -p meshing --lib java::fluid_tests`. GPU geometry/UV witnesses are in
`cargo test -p render --test it liquid_geometry`. Neither is vanilla gameplay evidence.


The liquid stream uses transparent water first and depth-writing lava last, matching
the shared GPU publication contract. Per-quad light indices survive that stable ordering.
A mixed pool in the real client exposed the ordering defect; the corrected build joined
and rendered the same scene. Do not remove the renderer's ordering assertion.

Controlled offline validation used vanilla 26.3, Java 25, Linux Xvfb and Mesa lavapipe.
Server administrative commands prepared source pools, falling columns, wet/dry bottom
slabs, lily pads, powered/unpowered redstone and tall grass/fern. The client received
ordinary packets; no mock world or official client was used. Source removal drained the
visible column without reload; changing the slab's waterlogged property removed only
its water; a stone inserted at x=15 beside x=16 remeshed the pool. Revisit after a
1,000-block teleport and a full disconnect/reconnect restored the scene. Normal forward
input changed the server's reported position. These are functional software-rendering
observations, not vanilla visual parity, hardware performance or iPhone qualification.
A chest then failed explicitly in the native special-renderer gate. Mixed-biome tint
boundaries and Java resource-pack reload were not qualified by this scene.
