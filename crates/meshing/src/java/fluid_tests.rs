//! CPU acceptance against verified official state/resource data. No server or screenshot claim.
use super::*;
use assets::registry::java::{BlockRegistry, catalog::BlockModels, fluids::Table, source::Jar};
use world::java::{BiomeId, BlockStateId, Light, Palette, Section, Terrain};

struct Fixture {
    registry: Arc<BlockRegistry>,
    graphics: Graphics,
    models: BlockModels,
}
impl Fixture {
    fn load() -> Option<Self> {
        Self::load_selected(false)
    }
    fn load_selected(omit_lava: bool) -> Option<Self> {
        let Some(root) = std::env::var_os("CINNABAR_JAVA_TEST_ASSETS") else {
            eprintln!("SKIP: missing CINNABAR_JAVA_TEST_ASSETS verified runtime fixtures");
            return None;
        };
        let root = std::path::PathBuf::from(root);
        let pin: serde_json::Value =
            serde_json::from_str(include_str!("../../../java-protocol/data/target.json")).unwrap();
        assert_eq!(pin["game_version"], "26.3");
        let hash = |key: &str| pin[key]["sha256"].as_str().unwrap();
        let registry = Arc::new(
            BlockRegistry::load(
                &std::fs::read(root.join("blocks.json")).unwrap(),
                hash("block_registry_report"),
            )
            .unwrap(),
        );
        let table = Arc::new(
            Table::load(
                &std::fs::read(root.join("fluids.json")).unwrap(),
                hash("fluid_render_report"),
                registry.len(),
            )
            .unwrap(),
        );
        let jar = Jar::open(
            &root.join("client.jar"),
            hash("client_assets"),
            pin["client_assets"]["bytes"].as_u64().unwrap(),
        )
        .unwrap();
        let mut models = BlockModels::new(registry.clone(), Box::new(jar));
        models.fluids = Some(table);
        let selected = (0..registry.len() as u32)
            .filter(|id| {
                (!omit_lava || &*registry.state(*id).unwrap().name != "minecraft:lava")
                    && matches!(
                        &*registry.state(*id).unwrap().name,
                        "minecraft:air"
                            | "minecraft:water"
                            | "minecraft:lava"
                            | "minecraft:stone"
                            | "minecraft:oak_slab"
                            | "minecraft:glass"
                            | "minecraft:barrier"
                    )
            })
            .collect::<Vec<_>>();
        let graphics =
            assets::registry::java::graphics::compile(&mut models, selected, 64).unwrap();
        Some(Self {
            registry,
            graphics,
            models,
        })
    }
    fn id(&self, name: &str, props: &[(&str, &str)]) -> u32 {
        (0..self.registry.len() as u32)
            .find(|id| {
                let s = self.registry.state(*id).unwrap();
                &*s.name == name
                    && props
                        .iter()
                        .all(|(k, v)| s.properties.get(*k).is_some_and(|s| s == v))
            })
            .unwrap()
    }
    fn terrain(&self) -> Terrain {
        let mut t = Terrain::default();
        t.begin(Dimension {
            session: 1,
            epoch: 1,
            name: "minecraft:overworld".into(),
            min_y: -64,
            height: 32,
            has_skylight: true,
        });
        for x in -1..=1 {
            for z in -1..=1 {
                t.replace([x, z], self.column());
            }
        }
        t
    }
    fn column(&self) -> Column {
        let sections = (0..2)
            .map(|_| {
                Arc::new(Section {
                    blocks: Palette::uniform(BlockStateId(self.id("minecraft:air", &[])), 4096),
                    biomes: Palette::uniform(BiomeId(0), 64),
                })
            })
            .collect();
        Column::new(
            sections,
            Light {
                sky: vec![Some(Arc::from(vec![0xff; 2048])); 4],
                block: vec![Some(Arc::from(vec![0; 2048])); 4],
            },
            vec![],
            vec![],
        )
    }
    fn mesh(&self, t: &Terrain, p: [i32; 3]) -> ChunkMesh {
        mesh(
            &Snapshot::capture(t, [p[0].div_euclid(16), p[2].div_euclid(16)]).unwrap(),
            p[1].div_euclid(16),
            &self.graphics,
        )
        .unwrap()
    }
}

#[test]
fn discovering_lava_keeps_resident_water_mesh_and_resource_addresses_valid() {
    let Some(mut f) = Fixture::load_selected(true) else {
        return;
    };
    let mut terrain = f.terrain();
    let p = [7, -56, 7];
    let water = f.id("minecraft:water", &[("level", "0")]);
    let lava = f.id("minecraft:lava", &[("level", "0")]);
    terrain.set_block(p, BlockStateId(water));
    let before = f.mesh(&terrain, p);
    let old = Arc::new(f.graphics);
    assert!(!old.contains(lava));
    let extension =
        assets::registry::java::graphics::extend(old.clone(), &mut f.models, [lava]).unwrap();
    assert!(extension.extends(&old));
    let next = extension.graphics();
    assert!(next.contains(lava));
    let water_materials = old
        .fluid_materials(assets::registry::java::fluids::Kind::Water)
        .unwrap();
    let retained = next
        .fluid_materials(assets::registry::java::fluids::Kind::Water)
        .unwrap();
    assert_eq!(retained.still, water_materials.still);
    assert_eq!(retained.flowing, water_materials.flowing);
    assert_eq!(retained.overlay, water_materials.overlay);
    let snapshot = Snapshot::capture(&terrain, [0, 0]).unwrap();
    let after = mesh(&snapshot, -4, next).unwrap();
    let words = |m: &ChunkMesh| {
        m.liquid_quads()
            .iter()
            .map(|q| q.words())
            .collect::<Vec<_>>()
    };
    assert_eq!(words(&before), words(&after));
    assert!(terrain.is_current(&snapshot.key));
}

#[test]
fn source_pool_slope_falling_and_materials_use_native_liquid_streams() {
    let Some(f) = Fixture::load() else { return };
    let mut t = f.terrain();
    let p = [7, -56, 7];
    let water = f.id("minecraft:water", &[("level", "0")]);
    t.set_block(p, BlockStateId(water));
    let m = f.mesh(&t, p);
    assert_eq!(m.liquid_quads().len(), 6);
    assert_eq!(
        f.graphics.presentation(water),
        Some(assets::registry::java::graphics::Presentation::Fluid)
    );
    assert!(
        m.model_refs().is_empty(),
        "liquid does not need an ordinary model"
    );
    let top = m
        .liquid_quads()
        .iter()
        .find(|q| q.face() == crate::Face::PositiveY)
        .unwrap();
    // Isolated source: weighted height (10 * 8/9) / 12; diagonal excluded.
    assert_eq!(top.heights(), [189; 4]);
    assert!(!top.is_depth_writing());
    for dx in -1..=1 {
        for dz in -1..=1 {
            t.set_block([p[0] + dx, p[1], p[2] + dz], BlockStateId(water));
        }
    }
    let m = f.mesh(&t, p);
    let center = m
        .liquid_quads()
        .iter()
        .find(|q| q.origin() == [7, 8, 7] && q.face() == crate::Face::PositiveY)
        .unwrap();
    assert_eq!(center.heights(), [227; 4]);
    t.set_block(
        [8, -56, 7],
        BlockStateId(f.id("minecraft:water", &[("level", "7")])),
    );
    let m = f.mesh(&t, p);
    let center = m
        .liquid_quads()
        .iter()
        .find(|q| q.origin() == [7, 8, 7] && q.face() == crate::Face::PositiveY)
        .unwrap();
    assert!(center.flow_gradient()[0] > 0);
    assert_eq!(center.flow_gradient()[1], 0);
    assert!(center.heights()[1] < center.heights()[0]);
    t.set_block(
        [7, -55, 7],
        BlockStateId(f.id("minecraft:water", &[("level", "8")])),
    );
    let m = f.mesh(&t, p);
    assert!(
        !m.liquid_quads()
            .iter()
            .any(|q| q.origin() == [7, 8, 7] && q.face() == crate::Face::PositiveY)
    );
    t.set_block(
        [4, -56, 4],
        BlockStateId(f.id("minecraft:lava", &[("level", "0")])),
    );
    let m = f.mesh(&t, p);
    let depth_start = m
        .liquid_quads()
        .iter()
        .position(|q| q.is_depth_writing())
        .unwrap();
    assert!(
        depth_start > 0,
        "the mixed section must retain transparent water"
    );
    assert!(
        m.liquid_quads()[depth_start..]
            .iter()
            .all(|q| q.is_depth_writing()),
        "GPU publication requires all lava after all water, independent of cell traversal"
    );
    let lava = m
        .liquid_quads()
        .iter()
        .find(|q| q.origin() == [4, 8, 4])
        .unwrap();
    assert!(lava.is_depth_writing());
    assert_eq!(
        m.liquid_lighting()[lava.lighting_index() as usize].samples()[0] & 15,
        15
    );
}

#[test]
fn host_occlusion_overlay_and_authoritative_replacement() {
    let Some(f) = Fixture::load() else { return };
    let mut t = f.terrain();
    let p = [7, -56, 7];
    let water = f.id("minecraft:water", &[("level", "0")]);
    t.set_block(p, BlockStateId(water));
    t.set_block([8, -56, 7], BlockStateId(f.id("minecraft:stone", &[])));
    assert!(
        !f.mesh(&t, p)
            .liquid_quads()
            .iter()
            .any(|q| q.face() == crate::Face::PositiveX)
    );
    t.set_block([8, -56, 7], BlockStateId(f.id("minecraft:glass", &[])));
    let m = f.mesh(&t, p);
    let side = m
        .liquid_quads()
        .iter()
        .find(|q| q.face() == crate::Face::PositiveX)
        .unwrap();
    assert!(!side.is_two_sided());
    assert_eq!(
        side.material_id(),
        f.graphics
            .fluid_materials(assets::registry::java::fluids::Kind::Water)
            .unwrap()
            .overlay
            .unwrap()
    );
    t.set_block(
        p,
        BlockStateId(f.id(
            "minecraft:oak_slab",
            &[("type", "bottom"), ("waterlogged", "true")],
        )),
    );
    let m = f.mesh(&t, p);
    assert!(!m.model_refs().is_empty());
    assert!(!m.liquid_quads().is_empty());
    assert!(
        !m.liquid_quads()
            .iter()
            .any(|q| q.face() == crate::Face::NegativeY)
    );
    let old = Snapshot::capture(&t, [0, 0]).unwrap();
    t.set_block(
        p,
        BlockStateId(f.id(
            "minecraft:oak_slab",
            &[("type", "bottom"), ("waterlogged", "false")],
        )),
    );
    assert!(!t.is_current(&old.key));
    assert!(f.mesh(&t, p).liquid_quads().is_empty());
    // An invisible host remains invisible while its contained water is visible.
    t.set_block([8, -56, 7], BlockStateId(f.id("minecraft:air", &[])));
    let wet_barrier = f.id("minecraft:barrier", &[("waterlogged", "true")]);
    t.set_block(p, BlockStateId(wet_barrier));
    let m = f.mesh(&t, p);
    assert!(m.model_refs().is_empty());
    assert!(!m.liquid_quads().is_empty());
    assert_eq!(
        f.graphics.presentation(wet_barrier),
        Some(assets::registry::java::graphics::Presentation::Fluid)
    );
    t.set_block(
        p,
        BlockStateId(f.id("minecraft:barrier", &[("waterlogged", "false")])),
    );
    let m = f.mesh(&t, p);
    assert!(m.model_refs().is_empty());
    assert!(m.liquid_quads().is_empty());
}

#[test]
fn boundaries_neighbor_arrival_and_removal_recover_without_session_leaks() {
    let Some(f) = Fixture::load() else { return };
    let mut t = f.terrain();
    let water = f.id("minecraft:water", &[("level", "0")]);
    t.set_block([-1, -49, 7], BlockStateId(water));
    t.unload([0, 0]);
    let missing = Snapshot::capture(&t, [-1, 0]).unwrap();
    assert!(
        f.mesh(&t, [-1, -49, 7])
            .liquid_quads()
            .iter()
            .any(|q| q.face() == crate::Face::PositiveX)
    );
    t.replace([0, 0], f.column());
    t.set_block([0, -49, 7], BlockStateId(water));
    assert!(!t.is_current(&missing.key));
    assert!(
        !f.mesh(&t, [-1, -49, 7])
            .liquid_quads()
            .iter()
            .any(|q| q.face() == crate::Face::PositiveX)
    );
    t.set_block([-1, -48, 7], BlockStateId(water));
    assert!(
        !f.mesh(&t, [-1, -49, 7])
            .liquid_quads()
            .iter()
            .any(|q| q.face() == crate::Face::PositiveY)
    );
    let old = Snapshot::capture(&t, [-1, 0]).unwrap();
    t.begin(Dimension {
        session: 2,
        epoch: 1,
        name: "minecraft:overworld".into(),
        min_y: -64,
        height: 32,
        has_skylight: true,
    });
    assert!(!t.is_current(&old.key));
}

#[test]
fn fluids_are_translation_invariant_and_different_fluids_keep_their_boundary() {
    let Some(f) = Fixture::load() else { return };
    let water = f.id("minecraft:water", &[("level", "0")]);
    let lava = f.id("minecraft:lava", &[("level", "0")]);
    let scene = |p: [i32; 3]| {
        let mut t = f.terrain();
        t.set_block(p, BlockStateId(water));
        t.set_block([p[0] + 1, p[1], p[2]], BlockStateId(lava));
        t
    };
    let p = [7, -56, 7];
    let a = scene(p);
    let b = scene([-9, -40, 7]);
    let ma = f.mesh(&a, p);
    let mb = f.mesh(&b, [-9, -40, 7]);
    assert_eq!(ma.liquid_quads(), mb.liquid_quads());
    assert_eq!(ma.liquid_lighting(), mb.liquid_lighting());
    assert!(
        ma.liquid_quads()
            .iter()
            .any(|q| q.origin() == [7, 8, 7] && q.face() == crate::Face::PositiveX)
    );
    assert!(
        ma.liquid_quads()
            .iter()
            .any(|q| q.origin() == [8, 8, 7] && q.face() == crate::Face::NegativeX)
    );
    let mut changed = a;
    let pending = Snapshot::capture(&changed, [0, 0]).unwrap();
    for level in [1, 7, 0, 3] {
        changed.set_block(
            p,
            BlockStateId(f.id("minecraft:water", &[("level", &level.to_string())])),
        );
    }
    assert!(!changed.is_current(&pending.key));
    assert_ne!(ma.liquid_quads(), f.mesh(&changed, p).liquid_quads());
}
