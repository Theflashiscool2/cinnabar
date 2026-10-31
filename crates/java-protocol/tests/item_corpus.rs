//! Batch wire coverage: failures are collected for every pinned component, never first-only.
use java_protocol::{codec, session::inventory::decode_stack, target};
use std::collections::BTreeSet;
#[path = "support/item_corpus.rs"]
mod corpus;

#[test]
fn every_pinned_component_decodes_and_preserves_the_following_component() {
    let fixtures = corpus::fixtures();
    assert_eq!(
        fixtures
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<BTreeSet<_>>(),
        target::ITEM_COMPONENTS.iter().map(|c| c.name).collect()
    );
    let mut errors = Vec::new();
    for (name, payload) in fixtures {
        let mut item = corpus::item(&name, &payload);
        item[2] = 2;
        let sentinel = if name == "minecraft:damage" {
            "minecraft:repair_cost"
        } else {
            "minecraft:damage"
        };
        codec::write_varint(
            target::ITEM_COMPONENTS
                .iter()
                .position(|c| c.name == sentinel)
                .unwrap() as i32,
            &mut item,
        );
        item.push(97);
        match decode_stack(&item) {
            Ok(Some(stack))
                if stack.added.contains_key(name.as_str()) && stack.added[sentinel] == 97 => {}
            result => errors.push(format!("{name}: {result:?}")),
        }
    }
    assert!(
        errors.is_empty(),
        "{} failures:\n{}",
        errors.len(),
        errors.join("\n")
    );
}

#[test]
fn corpus_truncation_and_trailing_junk_never_publish_partial_items() {
    let mut errors = Vec::new();
    for (name, payload) in corpus::fixtures() {
        let mut item = corpus::item(&name, &payload);
        for end in 0..item.len() {
            if decode_stack(&item[..end]).is_ok() {
                errors.push(format!("{name}: prefix {end}"));
            }
        }
        item.push(255);
        if decode_stack(&item).is_ok() {
            errors.push(format!("{name}: trailing junk"));
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn representative_server_menu_metadata_remains_structured_data() {
    let fixtures = corpus::fixtures();
    let value = |name: &str| {
        let (_, bytes) = fixtures.iter().find(|(n, _)| n == name).unwrap();
        decode_stack(&corpus::item(name, bytes))
            .unwrap()
            .unwrap()
            .added[name]
            .clone()
    };
    let profile = value("minecraft:profile");
    assert_eq!(profile["name"], "Fixture");
    assert_eq!(profile["properties"][0]["name"], "textures");
    assert_eq!(profile["properties"][0]["value"], "opaque fixture property");
    assert_eq!(profile["slim"], true);
    let model = value("minecraft:custom_model_data");
    assert_eq!(model["floats"][0], 1.25);
    assert_eq!(model["flags"], serde_json::json!([true, false]));
    assert_eq!(model["colors"][0], 0x123456);
    let potion = value("minecraft:potion_contents");
    assert_eq!(potion["effects"][0]["details"]["duration"], 200);
    assert_eq!(potion["name"], "Potion");
    let contents = value("minecraft:container");
    assert_eq!(contents[0]["item"], 55);
    assert_eq!(contents[0]["count"], 2);
    assert!(contents[1].is_null());
    assert_eq!(
        value("minecraft:writable_book_content")["pages"][0]["filtered"],
        "filtered"
    );
    assert_eq!(
        value("minecraft:written_book_content")["pages"][0]["raw"]["String"],
        "Page"
    );
    assert_eq!(value("minecraft:firework_explosion")["colors"][0], 0xff0000);
    assert_eq!(
        value("minecraft:sign_text_back")["messages"][3]["String"],
        "4"
    );
}

#[test]
fn recursive_templates_and_effects_have_a_depth_bound() {
    let id = target::ITEM_COMPONENTS
        .iter()
        .position(|c| c.name == "minecraft:use_remainder")
        .unwrap() as i32;
    let mut template = vec![55, 1, 0, 0];
    for _ in 0..70 {
        let mut outer = vec![55, 1, 1, 0];
        codec::write_varint(id, &mut outer);
        outer.extend(template);
        template = outer;
    }
    assert!(decode_stack(&corpus::item("minecraft:use_remainder", &template)).is_err());
    let mut details = vec![0, 0, 0, 0, 0, 0];
    for _ in 0..70 {
        let mut outer = vec![0, 0, 0, 0, 0, 1];
        outer.extend(details);
        details = outer;
    }
    let mut potion = vec![0, 0, 1, 0];
    potion.extend(details);
    potion.push(0);
    assert!(decode_stack(&corpus::item("minecraft:potion_contents", &potion)).is_err());
}

#[test]
fn conditional_profiles_effects_and_predicates_keep_their_fields() {
    use serde_json::json;
    let decode = |name, payload: &[u8]| {
        decode_stack(&corpus::item(name, payload))
            .unwrap()
            .unwrap()
            .added[name]
            .clone()
    };
    let text = |out: &mut Vec<u8>, value: &str| {
        codec::write_string(value, 32767, out).unwrap();
    };
    // Dynamic profile: optional name/UUID, signed property, absent skin keys, wide model.
    let mut profile = vec![0, 1];
    text(&mut profile, "Player");
    profile.push(1);
    profile.extend([0x12; 16]);
    profile.push(1);
    text(&mut profile, "textures");
    text(&mut profile, "inert");
    profile.push(1);
    text(&mut profile, "signature");
    profile.extend([0, 0, 0, 1, 0]);
    let value = decode("minecraft:profile", &profile);
    assert_eq!(value["dynamic"], true);
    assert_eq!(value["id"], "12121212-1212-1212-1212-121212121212");
    assert_eq!(value["properties"][0]["signature"], "signature");
    assert_eq!(value["slim"], false);
    // All five consume-effect alternatives, including a hidden effect and inline sound.
    let mut effects = vec![5, 0, 1, 3, 2, 120, 0, 1, 1, 1, 1, 60, 1, 0, 0, 0];
    effects.extend(0.75f32.to_be_bytes());
    effects.extend([1, 2, 3, 2, 3]); // remove holder-set [3], clear, teleport
    effects.extend(8f32.to_be_bytes());
    effects.extend([1, 4, 0]);
    text(&mut effects, "minecraft:fixture");
    effects.push(1);
    effects.extend(16f32.to_be_bytes());
    let value = decode("minecraft:death_protection", &effects);
    assert_eq!(value[0]["effects"][0]["details"]["hidden"]["duration"], 60);
    assert_eq!(value[0]["probability"], 0.75);
    assert_eq!(value[1]["effects"]["ids"], json!([3]));
    assert_eq!(value[2]["kind"], "clear");
    assert_eq!(value[3]["diameter"], 8.0);
    assert_eq!(value[4]["sound"]["inline"]["fixed_range"], 16.0);
    assert!(decode_stack(&corpus::item("minecraft:death_protection", &[1, 5])).is_err());
    // Predicate: absent blocks/properties/NBT, one exact component and one partial matcher.
    let damage = target::ITEM_COMPONENTS
        .iter()
        .position(|c| c.name == "minecraft:damage")
        .unwrap();
    let mut predicate = vec![1, 0, 0, 0, 1];
    codec::write_varint(damage as i32, &mut predicate);
    predicate.extend([7, 1, 1, 2, 10, 0]);
    let value = decode("minecraft:can_break", &predicate);
    assert_eq!(value[0]["components"]["exact"]["minecraft:damage"], 7);
    assert_eq!(value[0]["components"]["partial"][0]["id"], 2);
    assert_eq!(
        value[0]["components"]["partial"][0]["data"],
        json!({"Compound":{}})
    );
}

#[test]
fn expanded_components_are_bounded_independently_of_wire_size() {
    // Flags occupy one byte on wire but much more as retained JSON nodes.
    let mut model = vec![0];
    codec::write_varint(70_000, &mut model);
    model.extend(std::iter::repeat_n(1, 70_000));
    model.extend([0, 0]);
    let error = decode_stack(&corpus::item("minecraft:custom_model_data", &model)).unwrap_err();
    assert!(
        error.to_string().contains("expanded item list budget"),
        "{error}"
    );
    // A hostile count must fail before allocating based solely on that count.
    let oversized = [0, 0xff, 0xff, 0xff, 0xff, 7];
    assert!(decode_stack(&corpus::item("minecraft:custom_model_data", &oversized)).is_err());
}
