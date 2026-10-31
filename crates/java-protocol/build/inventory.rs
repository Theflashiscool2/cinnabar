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
