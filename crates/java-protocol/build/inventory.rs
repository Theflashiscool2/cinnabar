//! Derives native item/component/menu identities from the same pinned protocol revision.
use sha2::{Digest, Sha256};

fn source(pin: &serde_json::Value) -> String {
    let path = pin["path"].as_str().unwrap();
    println!("cargo:rerun-if-changed={path}");
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        pin["sha256"].as_str().unwrap()
    );
    String::from_utf8(bytes).unwrap()
}
fn validate(names: &[String], pin: &serde_json::Value) {
    assert_eq!(names.len() as u64, pin["count"].as_u64().unwrap());
    assert_eq!(
        names
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        names.len()
    );
    assert!(names.iter().all(|n| !n.is_empty()));
}
pub fn generate(code: &mut String, target: &serde_json::Value) {
    let schema = &target["inventory_schema"];
    let modifiers = source(&schema["attribute_modifiers"]);
    let operations = source(&schema["modifier_operations"]);
    for (text, name, constant, pin) in [
        (
            &modifiers,
            "EquipmentSlotGroup",
            "ITEM_ATTRIBUTE_SLOTS",
            &schema["attribute_modifiers"]["slots"],
        ),
        (
            &modifiers,
            "DisplayType",
            "ITEM_ATTRIBUTE_DISPLAYS",
            &schema["attribute_modifiers"]["display"],
        ),
        (
            &operations,
            "ModifierOperation",
            "ATTRIBUTE_OPERATIONS",
            &schema["modifier_operations"],
        ),
    ] {
        let values = text
            .split_once(&format!("public enum {name} {{"))
            .unwrap()
            .1
            .split_once(';')
            .unwrap()
            .0
            .split(',')
            .map(|s| s.trim().to_owned())
            .collect::<Vec<_>>();
        validate(&values, pin);
        *code += &format!("pub const {constant}: &[&str] = &{values:?};\n");
    }
    let items: Vec<String> = serde_json::from_str(&source(&schema["items"])).unwrap();
    validate(&items, &schema["items"]);
    *code += &format!("pub const ITEM_TYPES: &[&str] = &{items:?};\n");
    let native: serde_json::Value = serde_json::from_str(&source(&schema["native_rules"])).unwrap();
    assert_eq!(native["schema"], schema["native_rules"]["schema"]);
    assert_eq!(native["game_version"], target["game_version"]);
    assert_eq!(native["protocol"], target["protocol"]);
    assert_eq!(
        schema["native_rules"]["source_client_sha256"],
        target["client_assets"]["sha256"]
    );
    let entries = native["items"].as_array().unwrap();
    assert_eq!(entries.len(), items.len());
    assert_eq!(
        entries.len() as u64,
        schema["native_rules"]["items"].as_u64().unwrap()
    );
    let limits = entries
        .iter()
        .zip(&items)
        .map(|(entry, name)| {
            assert_eq!(entry[0].as_str().unwrap(), name);
            let limit = u32::try_from(entry[1].as_u64().unwrap()).unwrap();
            assert!(limit > 0);
            limit
        })
        .collect::<Vec<_>>();
    *code += &format!("pub const ITEM_STACK_LIMITS: &[u32] = &{limits:?};\n");
    let names = entries
        .iter()
        .map(|entry| {
            assert_eq!(entry.as_array().unwrap().len(), 4);
            assert!(entry[2].is_object() || entry[2].is_null() || entry[2].is_string());
            entry[2].to_string()
        })
        .collect::<Vec<_>>();
    *code += &format!("pub const ITEM_DEFAULT_NAMES: &[&str] = &{names:?};\n");
    *code += "#[derive(Clone, Copy, Debug)]\npub struct ItemAttackDefaults { pub range: bool, pub piercing: bool, pub minimum_charge: f64 }\npub const ITEM_ATTACK_DEFAULTS: &[ItemAttackDefaults] = &[\n";
    for entry in entries {
        let attack = &entry[3];
        let range = attack["range"].as_bool().unwrap();
        let piercing = attack["piercing"].as_bool().unwrap();
        let charge = attack["minimum_charge"].as_f64().unwrap();
        assert!(charge.is_finite() && charge >= 0.);
        *code += &format!(
            "ItemAttackDefaults {{ range: {range}, piercing: {piercing}, minimum_charge: {charge:?} }},\n"
        );
    }
    *code += "];\n";
    let uses: serde_json::Value = serde_json::from_str(&source(&schema["use_rules"])).unwrap();
    assert_eq!(uses["schema"], schema["use_rules"]["schema"]);
    assert_eq!(uses["game_version"], target["game_version"]);
    assert_eq!(uses["protocol"], target["protocol"]);
    assert_eq!(
        schema["use_rules"]["source_client_sha256"],
        target["client_assets"]["sha256"]
    );
    let entries = uses["items"].as_array().unwrap();
    assert_eq!(entries.len(), items.len());
    assert_eq!(
        entries.len() as u64,
        schema["use_rules"]["items"].as_u64().unwrap()
    );
    *code += "#[derive(Clone, Copy, Debug)]\npub struct ItemUseDefaults { pub duration: u32, pub group: &'static str }\npub const ITEM_USE_DEFAULTS: &[ItemUseDefaults] = &[\n";
    for (entry, name) in entries.iter().zip(&items) {
        assert_eq!(entry.as_array().unwrap().len(), 3);
        assert_eq!(entry[0].as_str().unwrap(), name);
        let duration = u32::try_from(entry[1].as_u64().unwrap()).unwrap();
        let group = entry[2].as_str().unwrap();
        *code += &format!("ItemUseDefaults {{ duration: {duration}, group: {group:?} }},\n");
    }
    *code += "];\n";
    let empty = u32::try_from(native["empty_item"].as_u64().unwrap()).unwrap();
    assert!((empty as usize) < items.len());
    *code += &format!("pub const ITEM_EMPTY_ID: u32 = {empty};\n");
    let slot_limit = u32::try_from(native["container_slot_limit"].as_u64().unwrap()).unwrap();
    assert!(slot_limit > 0);
    *code += &format!("pub const CONTAINER_SLOT_LIMIT: u32 = {slot_limit};\n");
    let max_changes = usize::try_from(native["max_changed_slots"].as_u64().unwrap()).unwrap();
    assert!(max_changes > 0);
    *code += &format!("pub const CONTAINER_MAX_CHANGED_SLOTS: usize = {max_changes};\n");
    let inputs = native["inputs"].as_object().unwrap();
    assert_eq!(
        inputs.len() as u64,
        schema["native_rules"]["inputs"].as_u64().unwrap()
    );
    let mut ids = std::collections::BTreeSet::new();
    for (name, id) in inputs {
        assert!(!name.is_empty() && name.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'));
        let id = u8::try_from(id.as_u64().unwrap()).unwrap();
        assert!(ids.insert(id));
        *code += &format!("pub const CONTAINER_INPUT_{name}: u8 = {id};\n");
    }
    let text = source(&schema["containers"]);
    let containers = text
        .split_once("public enum ContainerType {")
        .unwrap()
        .1
        .split_once(';')
        .unwrap()
        .0
        .split(',')
        .map(|s| s.trim().to_owned())
        .collect::<Vec<_>>();
    validate(&containers, &schema["containers"]);
    *code += &format!("pub const CONTAINER_TYPES: &[&str] = &{containers:?};\n");
    let text = source(&schema["components"]);
    let mut names = Vec::new();
    *code += "#[derive(Clone, Copy, Debug)]\npub struct ItemComponent { pub name: &'static str, pub reader: &'static str }\npub const ITEM_COMPONENTS: &[ItemComponent] = &[\n";
    for line in text.lines().filter(|l| l.contains(" = register(")) {
        let (_, rest) = line.split_once(", \"").unwrap();
        let (name, rest) = rest.split_once("\", ").unwrap();
        let reader = rest.split_once(", ").unwrap().0;
        let name = format!("minecraft:{name}");
        names.push(name.clone());
        *code += &format!("ItemComponent {{ name: {name:?}, reader: {reader:?} }},\n");
    }
    *code += "];\n";
    validate(&names, &schema["components"]);
}
