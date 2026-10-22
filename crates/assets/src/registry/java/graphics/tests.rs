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
    let report=serde_json::to_vec(&json!({"minecraft:stone":{"states":[{"id":0,"default":true}]},"minecraft:air":{"states":[{"id":1,"default":true}]}})).unwrap();
    let registry = BlockRegistry::load(&report, &format!("{:x}", Sha256::digest(&report))).unwrap();
    let faces = ["down", "up", "west", "east", "north", "south"]
        .into_iter()
        .map(|f| (f.to_owned(), json!({"texture":"#all","cullface":f})))
        .collect::<serde_json::Map<_, _>>();
    let files=BTreeMap::from([
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
