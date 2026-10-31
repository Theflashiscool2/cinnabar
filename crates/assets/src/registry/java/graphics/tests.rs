use super::*;
use crate::registry::java::{BlockRegistry, source::Source};
use serde_json::json;
use sha2::{Digest, Sha256};
struct Resources(BTreeMap<String, Vec<u8>>);
impl Source for Resources {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        self.0
            .get(path)
            .cloned()
            .ok_or(Error::Zip(zip::result::ZipError::FileNotFound))
    }
}
fn png(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(pixels)
            .unwrap();
    }
    bytes
}
fn models() -> BlockModels {
    let report=serde_json::to_vec(&json!({"minecraft:stone":{"states":[{"id":0,"default":true}]},"minecraft:air":{"states":[{"id":1,"default":true}]},"minecraft:dirt":{"states":[{"id":2,"default":true}]}})).unwrap();
    let registry = BlockRegistry::load(&report, &format!("{:x}", Sha256::digest(&report))).unwrap();
    let faces = ["down", "up", "west", "east", "north", "south"]
        .into_iter()
        .map(|f| (f.to_owned(), json!({"texture":"#all","cullface":f})))
        .collect::<serde_json::Map<_, _>>();
    let files=BTreeMap::from([
        ("assets/minecraft/blockstates/dirt.json".into(),br#"{"variants":{"":{"model":"block/dirt"}}}"#.to_vec()),
        ("assets/minecraft/models/block/dirt.json".into(),serde_json::to_vec(&json!({"textures":{"all":"block/larger"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":faces}]})).unwrap()),
        ("assets/minecraft/textures/block/larger.png".into(),png(&[90,70,30,255].repeat(4),2,2)),
        ("assets/minecraft/blockstates/stone.json".into(),serde_json::to_vec(&json!({"variants":{"":{"model":"block/stone"}}})).unwrap()),
        ("assets/minecraft/blockstates/air.json".into(),serde_json::to_vec(&json!({"variants":{"":{"model":"block/air"}}})).unwrap()),
        ("assets/minecraft/models/block/stone.json".into(),serde_json::to_vec(&json!({"textures":{"all":"block/test"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":faces}]})).unwrap()),
        ("assets/minecraft/models/block/air.json".into(),b"{\"elements\":[]}".to_vec()),
        ("assets/minecraft/textures/block/test.png".into(),png(&[40,60,80,255],1,1)),
    ]);
    BlockModels::new(Arc::new(registry), Box::new(Resources(files)))
}
#[test]
fn native_state_zero_is_stone_and_never_becomes_the_diagnostic_visual() {
    let compiled = compile(&mut models(), [0, 1], 64).unwrap();
    assert_eq!(compiled.runtime.visual_count(), 1); // no Java ID appended to Bedrock identities
    let selected = compiled.select(0, [-7, -62, 4]).unwrap();
    assert_eq!(selected.len(), 1);
    let template = &compiled.templates[&selected[0]];
    assert_eq!(template.occludes, 63);
    assert!(compiled.select(1, [0, 0, 0]).unwrap().is_empty());
    assert!(compiled.select(99, [0, 0, 0]).is_err());
    let gpu = compiled.runtime.model_templates()[selected[0] as usize];
    assert_eq!(gpu.quad_count, 6);
    let quad = compiled.runtime.model_quads()[gpu.quad_start as usize];
    assert_eq!(quad.material, 1);
    assert_eq!(quad.positions[0], [0, 0, 256]);
    assert_eq!(
        compiled.runtime.material(quad.material).texture,
        TextureRef::new(1, 0).unwrap()
    );
    assert_eq!(
        &*compiled.runtime.texture_pages()[1].texture.mips[0].rgba8,
        &[40, 60, 80, 255]
    );
}
#[test]
fn animation_reorders_real_frames_and_rejects_unknown_timing_semantics() {
    let id = Identifier::parse("block/test").unwrap();
    let image = png(&[1, 2, 3, 255, 4, 5, 6, 128], 1, 2);
    let mut source = Resources(BTreeMap::from([
        (id.asset_path("textures", "png"), image),
        (
            id.asset_path("textures", "png.mcmeta"),
            br#"{"animation":{"frametime":3,"frames":[1,0,1],"interpolate":true}}"#.to_vec(),
        ),
    ]));
    let sprite = Sprite::load(&mut source, &id).unwrap();
    assert_eq!(sprite.timeline, [1, 0, 1]);
    assert_eq!(sprite.ticks, 3);
    assert!(sprite.interpolate);
    assert_eq!(sprite.frames[1].rgba, [4, 5, 6, 128]);
    source.0.insert(
        id.asset_path("textures", "png.mcmeta"),
        br#"{"animation":{"frames":[{"index":0,"time":2},{"index":1,"time":3}]}}"#.to_vec(),
    );
    assert!(
        Sprite::load(&mut source, &id)
            .unwrap_err()
            .to_string()
            .contains("variable-duration")
    );
}
#[test]
fn grass_tint_is_explicit_and_unknown_callbacks_fail() {
    assert_eq!(
        tint("minecraft:grass_block", 0).unwrap(),
        crate::MATERIAL_FLAG_GRASS_TINT
    );
    assert_eq!(tint("minecraft:anything", -1).unwrap(), 0);
    assert!(tint("minecraft:redstone_wire", 0).is_err());
}

#[test]
fn extension_keeps_resident_draw_addresses_and_source_pixels_without_reordering_states() {
    for (first, added) in [(0, 2), (2, 0)] {
        let old = Arc::new(compile(&mut models(), [first, 1], 64).unwrap());
        let selected = old.select(first, [-3, -60, 9]).unwrap();
        let extension = extend(Arc::clone(&old), &mut models(), [added, first]).unwrap();
        let next = extension.graphics();
        assert!(extension.extends(&old));
        assert!(!extension.extends(next));
        assert!(!old.contains(added));
        assert!(next.contains(added));
        assert_eq!(next.select(first, [-3, -60, 9]).unwrap(), selected);
        assert!(
            next.runtime
                .model_templates()
                .starts_with(old.runtime.model_templates())
        );
        assert!(
            next.runtime
                .model_quads()
                .starts_with(old.runtime.model_quads())
        );
        assert!(
            next.runtime
                .materials()
                .starts_with(old.runtime.materials())
        );
        assert_eq!(
            next.runtime.texture_pages().len(),
            2,
            "reuse partially occupied pages"
        );
        let reference = TextureRef::new(1, 0).unwrap();
        assert_eq!(
            next.runtime.texture_source_size(reference),
            old.runtime.texture_source_size(reference)
        );
        let old_pixel = &old.runtime.texture_pages()[1].texture.mips[0].rgba8[..4];
        assert_eq!(
            &next.runtime.texture_pages()[1].texture.mips[0].rgba8[..4],
            old_pixel
        );
        assert!(extension.resources().matches(&old.runtime, &next.runtime));
        let duplicate = extend(Arc::clone(next), &mut models(), [added]).unwrap();
        assert_eq!(
            duplicate.graphics().runtime.model_templates(),
            next.runtime.model_templates()
        );
        assert_eq!(
            duplicate.graphics().runtime.texture_pages(),
            next.runtime.texture_pages()
        );
    }
}
#[test]
fn extension_does_not_turn_unknown_models_into_air_or_damage_the_old_catalog() {
    let old = Arc::new(compile(&mut models(), [0], 64).unwrap());
    assert!(extend(Arc::clone(&old), &mut models(), [99]).is_err());
    assert!(!old.contains(99));
    assert!(!old.select(0, [0; 3]).unwrap().is_empty());
}

struct EmptyModels {
    cancelled: bool,
}
impl Source for EmptyModels {
    fn cancelled(&self) -> bool {
        self.cancelled
    }
    fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        assert!(
            !self.cancelled,
            "cancelled jobs must not read more resources"
        );
        Ok(if path.contains("/blockstates/") {
            br#"{"variants":{"":{"model":"block/empty"}}}"#.to_vec()
        } else {
            br#"{"elements":[]}"#.to_vec()
        })
    }
}

fn empty_models(cancelled: bool) -> BlockModels {
    let bytes = br#"{"minecraft:air":{"states":[{"id":0,"default":true}]},"minecraft:lava":{"states":[{"id":1,"default":true},{"id":2}]},"minecraft:water":{"states":[{"id":3,"default":true}]},"minecraft:chest":{"states":[{"id":4,"default":true}]}}"#;
    let registry = BlockRegistry::load(bytes, &format!("{:x}", Sha256::digest(bytes))).unwrap();
    BlockModels::new(Arc::new(registry), Box::new(EmptyModels { cancelled }))
}

#[test]
fn admission_batches_independent_failures_without_turning_special_models_into_air() {
    let Error::ModelBatch(report) = compile(&mut empty_models(false), [0, 1, 2, 3, 4, 99], 64)
        .err()
        .unwrap()
    else {
        panic!("expected grouped model failures");
    };
    assert_eq!(report.checked_states, 6);
    assert_eq!(report.failed_states, 5);
    assert_eq!(report.issues.len(), 4);
    assert_eq!(report.issues[0].block, "minecraft:lava");
    assert_eq!(report.issues[0].affected_states, 2);
    assert_eq!(report.issues[1].block, "minecraft:water");
    assert_eq!(report.issues[2].block, "minecraft:chest");
    assert_eq!(report.issues[3].sample_state_ids, [99]);
    assert!(
        report.issues[..3]
            .iter()
            .all(|i| i.reason.contains("special renderer"))
    );
    let valid = compile(&mut empty_models(false), [0], 64).unwrap();
    assert!(valid.select(0, [0; 3]).unwrap().is_empty());
    let valid = Arc::new(valid);
    assert!(matches!(
        extend(Arc::clone(&valid), &mut empty_models(false), [1, 3]),
        Err(Error::ModelBatch(_))
    ));
    assert!(!valid.contains(1));
    assert!(!valid.contains(3));
}

#[test]
fn batch_checks_stop_on_cancellation_instead_of_reporting_more_missing_features() {
    let error = compile(&mut empty_models(true), [0, 1, 2, 3, 4], 64)
        .err()
        .unwrap();
    assert!(!matches!(error, Error::ModelBatch(_)));
    assert!(error.to_string().contains("cancelled"));
}
