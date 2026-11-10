use super::*;
struct Memory(BTreeMap<String, Vec<u8>>);
impl Source for Memory {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        self.0
            .get(path)
            .cloned()
            .ok_or(Error::Zip(zip::result::ZipError::FileNotFound))
    }
    fn entries(&mut self, prefix: &str) -> Result<Vec<String>, Error> {
        Ok(self
            .0
            .keys()
            .filter(|p| p.starts_with(prefix))
            .cloned()
            .collect())
    }
}
fn png(width: u32, color: [u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, width);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&color.repeat((width * width) as usize))
            .unwrap();
    }
    bytes
}
fn catalog(definition: Value) -> Catalog {
    let mut files = BTreeMap::new();
    files.insert(
        "assets/minecraft/items/stone.json".into(),
        serde_json::to_vec(&serde_json::json!({"model":definition})).unwrap(),
    );
    for (name, color) in [("red", [255, 0, 0, 255]), ("blue", [0, 0, 255, 255])] {
        files.insert(format!("assets/minecraft/models/item/{name}.json"),serde_json::to_vec(&serde_json::json!({"parent":"minecraft:item/generated","textures":{"layer0":format!("minecraft:item/{name}")}})).unwrap());
        files.insert(
            format!("assets/minecraft/textures/item/{name}.png"),
            png(2, color),
        );
    }
    files.insert(
        "assets/minecraft/models/item/generated.json".into(),
        br#"{"parent":"minecraft:builtin/generated","gui_light":"front"}"#.to_vec(),
    );
    files.insert(
        "assets/minecraft/textures/colormap/grass.png".into(),
        png(256, [10, 20, 30, 255]),
    );
    Catalog::load(&mut Memory(files), &["minecraft:stone"]).unwrap()
}
fn stack() -> Stack {
    Stack {
        item: "minecraft:stone".into(),
        count: 1,
        added: Default::default(),
        removed: Default::default(),
    }
}
fn leaf(name: &str) -> Value {
    serde_json::json!({"type":"minecraft:model","model":format!("minecraft:item/{name}")})
}
fn middle(image: &Image) -> &[u8] {
    let offset = ((image.height / 2 * image.width + image.width / 2) * 4) as usize;
    &image.rgba[offset..offset + 4]
}
#[test]
fn generated_model_reads_inherited_texture_and_preserves_alpha() {
    let c = catalog(leaf("red"));
    let image = c.icon(&stack()).unwrap();
    let p = middle(&image);
    assert!(p[0] > 100);
    assert_eq!(&p[1..], [0, 0, 255]);
    let geometry = c.geometry(&stack()).unwrap();
    assert!(
        geometry.len() > 2,
        "generated front/back and alpha-edge extrusion"
    );
    assert!(
        geometry
            .iter()
            .flat_map(|q| q.positions.iter().flatten())
            .all(|v| v.is_finite())
    );
}
#[test]
fn count_dispatch_and_authoritative_model_override_are_not_canonical_name_guesses() {
    let c = catalog(
        serde_json::json!({"type":"minecraft:range_dispatch","property":"minecraft:count","normalize":false,"fallback":leaf("red"),"entries":[{"threshold":5,"model":leaf("blue")}]}),
    );
    let mut a = stack();
    a.added
        .insert("minecraft:max_stack_size".into(), serde_json::json!(64));
    let first = c.icon(&a).unwrap();
    assert!(middle(&first)[0] > 100);
    a.count = 5;
    let second = c.icon(&a).unwrap();
    assert!(middle(&second)[2] > 100);
    assert_eq!(middle(&second)[0], 0);
    a.added.insert(
        "minecraft:item_model".into(),
        Value::String("minecraft:not_present".into()),
    );
    assert!(c.icon(&a).is_err());
    a.added.clear();
    a.removed.insert("minecraft:item_model".into());
    assert!(c.geometry(&a).unwrap().is_empty());
}
#[test]
fn component_tint_and_keys_follow_authority_without_changing_stack() {
    let c = catalog(
        serde_json::json!({"type":"minecraft:model","model":"minecraft:item/red","tints":[{"type":"minecraft:constant","value":-16711936}]}),
    );
    let mut a = stack();
    let original = a.clone();
    let k = a.key();
    let image = c.icon(&a).unwrap();
    assert_eq!(middle(&image), [0, 0, 0, 255]);
    assert_eq!(a.key(), k);
    assert_eq!(a.item, original.item);
    a.added.insert(
        "minecraft:custom_model_data".into(),
        serde_json::json!({"strings":["alternate"]}),
    );
    assert_ne!(a.key(), k);
    a = original;
    a.count += 1;
    assert_ne!(a.key(), k);
}
#[test]
fn special_renderer_is_reported_rather_than_baked_as_missing_cube() {
    let c = catalog(
        serde_json::json!({"type":"minecraft:special","base":"minecraft:item/red","model":{"type":"minecraft:chest","texture":"minecraft:normal"}}),
    );
    assert!(
        c.icon(&stack())
            .unwrap_err()
            .to_string()
            .contains("special renderer")
    );
}

#[test]
fn native_defaults_respect_removed_components_and_potion_wire_registry() {
    use serde_json::json;
    // These valid native potions have no effects and therefore no optional color.
    for id in 0..4 {
        assert!(defaults::validate_potion(
            &json!({"potion":id,"effects":[]})
        ));
        assert_eq!(defaults::potion_color(&json!({"potion":id})), None);
    }
    assert!(defaults::is_plain_water(&json!({"potion":0})).unwrap());
    assert!(!defaults::is_plain_water(&json!({"potion":0,"effects":[{"id":0}]})).unwrap());
    assert_eq!(
        defaults::component("minecraft:diamond_sword", "minecraft:max_damage"),
        Some(&json!(1561))
    );
    assert_eq!(
        defaults::potion_color(&json!({"potion":24,"color":null,"effects":[]})),
        Some(-515037)
    );
    assert_eq!(
        defaults::potion_color(&json!({"potion":"minecraft:healing"})),
        Some(-515037)
    );
    // Pinned 26.3 PotionContents.getColorOptional: healing + speed II. A hidden
    // strength V must not affect the weighted result (independent native oracle).
    let wire = json!({"potion":24,"effects":[{"id":0,"details":{"amplifier":1,"particles":true}},{"id":4,"details":{"amplifier":4,"particles":false}}]});
    assert!(defaults::validate_potion(&wire));
    assert_eq!(defaults::potion_color(&wire), Some(-9131851));
    let persistent = json!({"potion":"minecraft:healing","custom_effects":[{"id":"minecraft:speed","amplifier":1}]});
    assert_eq!(defaults::potion_color(&persistent), Some(-9131851));
    assert_eq!(
        defaults::potion_color(&json!({"effects":[{"id":4,"details":{"particles":false}}]})),
        None
    );
    assert_eq!(
        defaults::potion_color(&json!({"potion":24,"color":0x123456})),
        Some(0x123456)
    );
    assert!(!defaults::validate_potion(&json!({"potion":123456})));
    assert!(!defaults::validate_potion(
        &json!({"effects":[{"id":123456}]})
    ));
    let c = catalog(leaf("red"));
    let mut a = stack();
    a.removed.insert("minecraft:item_model".into());
    assert!(c.geometry(&a).unwrap().is_empty());
}

fn native_item_types() -> Vec<String> {
    serde_json::from_slice(include_bytes!(
        "../../../../../java-protocol/data/item-types.json"
    ))
    .unwrap()
}
fn native_catalog() -> Option<Catalog> {
    let Some(root) = std::env::var_os("CINNABAR_JAVA_ASSETS") else {
        eprintln!(
            "missing fixture: CINNABAR_JAVA_ASSETS with verified pinned client.jar/models.json"
        );
        return None;
    };
    let root = std::path::Path::new(&root);
    let target: Value = serde_json::from_slice(include_bytes!(
        "../../../../../java-protocol/data/target.json"
    ))
    .unwrap();
    let mut source = super::super::source::Jar::open(
        &root.join("client.jar"),
        target["client_assets"]["sha256"].as_str().unwrap(),
        target["client_assets"]["bytes"].as_u64().unwrap(),
    )
    .unwrap();
    let models = crate::entity::java::Models::load(
        &std::fs::read(root.join("models.json")).unwrap(),
        target["entity_models_report"]["sha256"].as_str().unwrap(),
        target["entity_models_report"]["bytes"].as_u64().unwrap(),
    )
    .unwrap();
    Some(
        Catalog::load(
            &mut source,
            &native_item_types()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )
        .unwrap()
        .with_entity_models(Arc::new(models)),
    )
}
#[test]
fn native_special_item_geometry_and_component_changes_use_real_materials() {
    use serde_json::json;
    let Some(c) = native_catalog() else {
        return;
    };
    let make = |name: &str| Stack {
        item: format!("minecraft:{name}"),
        ..stack()
    };
    for name in [
        "chest",
        "trapped_chest",
        "ender_chest",
        "red_shulker_box",
        "white_banner",
        "skeleton_skull",
        "piglin_head",
        "conduit",
        "decorated_pot",
        "copper_golem_statue",
        "shield",
        "trident",
    ] {
        let geometry = c.geometry(&make(name)).unwrap();
        assert!(!geometry.is_empty(), "{name}");
        assert!(
            geometry
                .iter()
                .flat_map(|q| q.positions.iter().flatten())
                .all(|n| n.is_finite()),
            "{name}"
        );
        assert!(
            c.icon(&make(name))
                .unwrap()
                .rgba
                .chunks_exact(4)
                .any(|p| p[3] != 0),
            "{name}"
        );
    }
    let mut banner = make("white_banner");
    let plain = c.icon(&banner).unwrap().rgba;
    banner.added.insert(
        "minecraft:banner_patterns".into(),
        json!([{"pattern":{"asset_id":"minecraft:stripe_bottom"},"color":14}]),
    );
    assert_ne!(c.icon(&banner).unwrap().rgba, plain);
    let mut pot = make("decorated_pot");
    let plain = c.icon(&pot).unwrap().rgba;
    let sherd = native_item_types()
        .iter()
        .position(|n| *n == "minecraft:angler_pottery_sherd")
        .unwrap();
    pot.added.insert(
        "minecraft:pot_decorations".into(),
        json!({"sides":[null,null,null,{"item":sherd,"added":{},"removed":[]}]}),
    );
    let decorated = c.icon(&pot).unwrap().rgba;
    assert_ne!(decorated, plain);
    pot.added.get_mut("minecraft:pot_decorations").unwrap()["sides"][3]["removed"] =
        json!(["minecraft:provides_pottery_pattern"]);
    assert_eq!(c.icon(&pot).unwrap().rgba, plain);
    let mut head = make("player_head");
    let profile = json!({"id":"00112233-4455-6677-8899-aabbccddeeff","properties":[]});
    head.added
        .insert("minecraft:profile".into(), profile.clone());
    assert!(
        c.icon(&head).is_err(),
        "unresolved profile must not silently replace its material"
    );
    let mut skins = HeadTextures::default();
    skins.textures.insert(
        profile_key(&profile),
        HeadTexture::Image(Arc::new(Image {
            width: 64,
            height: 64,
            rgba: vec![20, 240, 70, 255].repeat(64 * 64),
        })),
    );
    let resolved = c.icon_with_heads(&head, &skins).unwrap();
    assert!(
        resolved
            .rgba
            .chunks_exact(4)
            .any(|p| p[1] > p[0] && p[1] > p[2] && p[3] > 0)
    );
}

#[test]
fn native_first_person_selects_real_models_instead_of_gui_sprites() {
    let Some(c) = native_catalog() else {
        return;
    };
    let heads = HeadTextures::default();
    for name in [
        "diamond_sword",
        "bow",
        "trident",
        "shield",
        "chest",
        "white_banner",
        "decorated_pot",
    ] {
        let item = Stack {
            item: format!("minecraft:{name}"),
            ..stack()
        };
        for view in [View::FirstPersonRight, View::FirstPersonLeft] {
            let quads = c.geometry_in(&item, &heads, view).unwrap();
            assert!(!quads.is_empty(), "{name} {view:?}");
            assert!(
                quads
                    .iter()
                    .flat_map(|q| q.positions.iter().flatten())
                    .all(|v| v.is_finite())
            );
        }
    }
    let trident = Stack {
        item: "minecraft:trident".into(),
        ..stack()
    };
    let root = std::env::var_os("CINNABAR_JAVA_ASSETS").unwrap();
    let target: Value = serde_json::from_slice(include_bytes!(
        "../../../../../java-protocol/data/target.json"
    ))
    .unwrap();
    let mut jar = super::super::source::Jar::open(
        &std::path::Path::new(&root).join("client.jar"),
        target["client_assets"]["sha256"].as_str().unwrap(),
        target["client_assets"]["bytes"].as_u64().unwrap(),
    )
    .unwrap();
    let expected = Image::decode(
        &jar.read("assets/minecraft/textures/entity/trident/trident.png")
            .unwrap(),
    )
    .unwrap();
    let held = c
        .geometry_in(&trident, &heads, View::FirstPersonRight)
        .unwrap();
    assert!(held.iter().all(|q| q.image.rgba == expected.rgba));
    assert!(
        c.geometry(&trident)
            .unwrap()
            .iter()
            .any(|q| q.image.rgba != expected.rgba)
    );
}

#[test]
fn ground_models_use_their_own_display_transform_instead_of_gui_or_hand() {
    let mut files = BTreeMap::new();
    files.insert(
        "assets/minecraft/items/stone.json".into(),
        serde_json::to_vec(&serde_json::json!({"model":leaf("drop")})).unwrap(),
    );
    files.insert("assets/minecraft/models/item/drop.json".into(), br#"{
        "parent":"minecraft:item/generated","textures":{"layer0":"minecraft:item/drop"},
        "display":{"ground":{"translation":[0,4,0],"scale":[0.25,0.25,0.25]},"gui":{"scale":[1,1,1]}}
    }"#.to_vec());
    files.insert(
        "assets/minecraft/models/item/generated.json".into(),
        br#"{"parent":"minecraft:builtin/generated"}"#.to_vec(),
    );
    files.insert(
        "assets/minecraft/textures/item/drop.png".into(),
        png(2, [255; 4]),
    );
    files.insert(
        "assets/minecraft/textures/colormap/grass.png".into(),
        png(256, [10, 20, 30, 255]),
    );
    let catalog = Catalog::load(&mut Memory(files), &["minecraft:stone"]).unwrap();
    let extent = |view| {
        let quads = catalog
            .geometry_in(&stack(), &HeadTextures::default(), view)
            .unwrap();
        assert!(!quads.is_empty());
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in quads.iter().flat_map(|q| q.positions) {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        (min, max)
    };
    let (min, max) = extent(View::Ground);
    // Native model translation is in sixteenths; the generated face is centered on the origin.
    assert!((min[0] + 0.125).abs() < 1e-6 && (max[0] - 0.125).abs() < 1e-6);
    assert!((min[1] - 0.125).abs() < 1e-6 && (max[1] - 0.375).abs() < 1e-6);
    let (min, max) = extent(View::Gui);
    assert!((max[0] - min[0] - 1.).abs() < 1e-6);
}

#[test]
fn worn_player_heads_use_raw_skull_geometry_and_authoritative_profile() {
    let Some(c) = native_catalog() else {
        return;
    };
    let mut head = Stack {
        item: "minecraft:player_head".into(),
        ..stack()
    };
    let profile = serde_json::json!({"id":"00112233-4455-6677-8899-aabbccddeeff","properties":[]});
    head.added
        .insert("minecraft:profile".into(), profile.clone());
    assert!(c.worn_player_head(&head, &HeadTextures::default()).is_err());
    let image = Arc::new(Image {
        width: 64,
        height: 64,
        rgba: vec![20, 240, 70, 255].repeat(64 * 64),
    });
    let heads = HeadTextures {
        revision: 1,
        textures: [(profile_key(&profile), HeadTexture::Image(image.clone()))].into(),
    };
    let raw = c.worn_player_head(&head, &heads).unwrap().unwrap();
    assert_eq!(raw.len(), 12, "native head and hat cubes");
    assert!(raw.iter().all(|quad| Arc::ptr_eq(&quad.image, &image)));
    // 26.3 SkullModel.createHumanoidHeadLayer: [-4,-8,-4], size 8, hat deformation .25.
    let low: [f32; 3] = std::array::from_fn(|axis| {
        raw.iter()
            .flat_map(|q| q.positions.iter())
            .map(|p| p[axis])
            .fold(f32::INFINITY, f32::min)
    });
    let high: [f32; 3] = std::array::from_fn(|axis| {
        raw.iter()
            .flat_map(|q| q.positions.iter())
            .map(|p| p[axis])
            .fold(f32::NEG_INFINITY, f32::max)
    });
    assert_eq!(low, [-4.25 / 16., -8.25 / 16., -4.25 / 16.]);
    assert_eq!(high, [4.25 / 16., 0.25 / 16., 4.25 / 16.]);
    // A custom item-model component must not replace the native skull worn-head branch.
    head.added.insert(
        "minecraft:item_model".into(),
        serde_json::json!("minecraft:diamond_sword"),
    );
    assert_eq!(
        c.worn_player_head(&head, &heads).unwrap().unwrap()[0].positions,
        raw[0].positions
    );
    head.removed.insert("minecraft:profile".into());
    assert!(
        c.worn_player_head(&head, &HeadTextures::default())
            .unwrap()
            .is_some()
    );
    assert!(c.worn_player_head(&stack(), &heads).unwrap().is_none());
}
