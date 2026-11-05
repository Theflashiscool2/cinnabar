use super::super::source::Error;
use super::*;
struct Memory(BTreeMap<String, Vec<u8>>);
impl Source for Memory {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        self.0
            .get(path)
            .cloned()
            .ok_or(Error::Zip(zip::result::ZipError::FileNotFound))
    }
    fn entries(&mut self, _: &str) -> Result<Vec<String>, Error> {
        Ok(self.0.keys().cloned().collect())
    }
}
fn source() -> Memory {
    let mut bytes = Vec::new();
    {
        let mut e = png::Encoder::new(&mut bytes, 1, 1);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .unwrap()
            .write_image_data(&[200, 100, 50, 128])
            .unwrap();
    }
    Memory([("assets/fixture/equipment/dyed.json".into(),br#"{"layers":{"humanoid":[{"texture":"fixture:colored","dyeable":{"color_when_undyed":16777215}}],"humanoid_leggings":[{"texture":"fixture:colored"}]}}"#.to_vec()),("assets/fixture/textures/entity/equipment/humanoid/colored.png".into(),bytes.clone()),("assets/fixture/textures/entity/equipment/humanoid_leggings/colored.png".into(),bytes)].into())
}
#[test]
fn native_equippable_slot_override_dye_and_removal_use_resource_layers() {
    let mut s = Stack {
        item: "minecraft:stone".into(),
        count: 1,
        added: Default::default(),
        removed: Default::default(),
    };
    s.added.insert(
        "minecraft:equippable".into(),
        serde_json::json!({"slot":4,"model":"fixture:dyed"}),
    );
    s.added
        .insert("minecraft:dyed_color".into(), serde_json::json!(0x80ff00));
    let a = armor(&mut source(), &s, 4, true).unwrap().unwrap();
    assert_eq!(a.model, "armor/player_slim/chestplate");
    assert_eq!(a.layers[0].rgba, [100, 100, 0, 128]);
    assert!(armor(&mut source(), &s, 3, false).unwrap().is_none());
    s.added.insert(
        "minecraft:equippable".into(),
        serde_json::json!({"slot":"legs","asset_id":"fixture:dyed"}),
    );
    let a = armor(&mut source(), &s, 3, false).unwrap().unwrap();
    assert_eq!(a.model, "armor/player/leggings");
    assert_eq!(a.layers[0].rgba, [200, 100, 50, 128]);
    s.removed.insert("minecraft:equippable".into());
    assert!(armor(&mut source(), &s, 3, false).unwrap().is_none());
    s.removed.clear();
    s.added.insert(
        "minecraft:equippable".into(),
        serde_json::json!({"slot":3,"model":"../../private"}),
    );
    assert!(armor(&mut source(), &s, 3, false).is_err());
}
