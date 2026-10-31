# Native Java block colors and empty models

Scope: the release pinned by `crates/java-protocol/data/target.json` (26.3).
Original bindings in `assets::registry::java::graphics::tints` are based on inspection
of the hash-verified official client archive's `BlockColors.createDefault`,
`getTintSource`, `BlockTintSources`, `Biome.getDryFoliageColor`, and the blockstate/model
resources. Reference code and game resources are not redistributed. The binding test
requires reinspection when the target version changes.

A model tint index selects a registered callback layer. No callback, a negative
index, or an out-of-range index leaves the texture unmodified. In particular,
cherry/pale oak leaves, bamboo, stonecutters and lava/powder-snow cauldrons do not
gain a grass tint merely because their model inherits a tint index. Petals and
wildflowers register unmodified layer zero and grass layer one.

Implemented bindings use the existing shared material/shader paths: grass,
foliage, biome water for cauldrons, fixed birch/spruce foliage, and stem age colors.
The Java palette supplies the fixed foliage colors in every biome, rather than
using Bedrock biome colors. Leaf litter uses the authoritative optional biome
`dry_foliage_color`, otherwise the client archive's dry foliage climate colormap.
Attached stems use the mature stem color. Material selection follows authoritative
state properties; append-only graphics extensions preserve old draw addresses.

Upper tall grass/large fern carry a quad tint callback that samples the block below,
including the previous section's lattice halo. Redstone carries authoritative power
and uses the reference's quantized gamma vertex color; lily pads use the in-world
constant, separately from their item/default color. Colors multiply sampled pixels
in the shaders rather than baking another quantization into the atlas. The shared
native, enhanced and shaderpack model paths use the same callback selection.
Nonuniform Java biome fields and biome grass modifiers remain gated separately;
these bindings do not establish complete sampling, blending or resource-pack parity.

Known empty terrain models are air, structure void, non-waterlogged barrier/light,
and upper pitcher crop ages zero through two. Reference: `BarrierBlock`, `LightBlock`
and `StructureVoidBlock.getRenderShape` plus the three empty pitcher-top resources.
Invisible terrain does not imply no collision. Waterlogged invisible hosts use the separate [native fluid path](java-fluids.md).
Chests, portals and other unsupported special renderer types are not converted into
invisible blocks.

Run focused deterministic tests:

```sh
cargo test --locked -p assets --lib registry::java::graphics
cargo test --locked -p client-world --lib java::biomes
```

For additional CPU model/material/texture compilation using the official archive,
set `CINNABAR_JAVA_CLIENT_TEST` to the pinned `client.jar` before the first command.
That test validates the archive hash and uses original synthetic state IDs and
properties. It skips with a named missing-fixture message when unavailable. It is
neither a whole-registry audit nor rendered/live evidence. `make java-audit` remains
the complete registry check when both pinned runtime files are prepared.

Visual acceptance is pending: inspect foliage in contrasting biomes, all stem ages,
cauldrons, petals, litter with biome override and fallback, and invisible-host
collision. Repeat authoritative replacements, resource reload and reconnect.
