//! Checks maintained serializer identities against hash-pinned runtime accessor schemas.
use sha2::{Digest, Sha256};
fn load(pin: &serde_json::Value) -> Vec<u8> {
    let path = pin["path"].as_str().unwrap();
    println!("cargo:rerun-if-changed={path}");
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        pin["sha256"].as_str().unwrap(),
        "Java metadata source drift"
    );
    if let Some(size) = pin["bytes"].as_u64() {
        assert_eq!(bytes.len() as u64, size);
    }
    bytes
}
pub fn generate(code: &mut String, target: &serde_json::Value, entity_names: &[String]) {
    let models = &target["entity_models_report"];
    *code += &format!(
        "pub const ENTITY_MODELS_REPORT_SHA256: &str = {:?};\npub const ENTITY_MODELS_REPORT_PATH: &str = {:?};\npub const ENTITY_MODELS_REPORT_BYTES: u64 = {};\n",
        models["sha256"].as_str().unwrap(),
        models["path"].as_str().unwrap(),
        models["bytes"].as_u64().unwrap()
    );
    let source = String::from_utf8(load(&target["metadata_schema"])).unwrap();
    let names = source
        .lines()
        .filter_map(|line| {
            let (declaration, _) = line.split_once(" = register(")?;
            Some(declaration.split_whitespace().last().unwrap())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names.len() as u64,
        target["metadata_schema"]["count"].as_u64().unwrap()
    );
    assert_eq!(
        names
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        names.len()
    );
    *code += &format!("pub const METADATA_SERIALIZERS: &[&str] = &{names:?};\n");
    let facts: serde_json::Value =
        serde_json::from_slice(&load(&target["entity_facts_report"])).unwrap();
    *code += "#[derive(Clone, Copy, Debug)]\npub struct MetadataSpec { pub index: u8, pub name: &'static str, pub serializer: usize }\n";
    for (kind, entries) in facts["metadata"].as_object().unwrap() {
        assert!(kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
        *code += &format!(
            "pub const METADATA_{}: &[MetadataSpec] = &[\n",
            kind.to_ascii_uppercase()
        );
        let mut seen = std::collections::BTreeSet::new();
        for entry in entries.as_array().unwrap() {
            let index = entry["index"].as_u64().unwrap();
            let serializer = entry["serializer"].as_u64().unwrap() as usize;
            assert!(index < 255 && seen.insert(index) && serializer < names.len());
            let name = entry["name"].as_str().unwrap();
            *code += &format!(
                "MetadataSpec {{ index: {index}, name: {name:?}, serializer: {serializer} }},\n"
            );
        }
        *code += "];\n";
    }
    *code += &format!(
        "pub const PIG_DEFAULT_VARIANT: &str = {:?};\n",
        facts["pig_default_variant"].as_str().unwrap()
    );
    let poses = facts["poses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>();
    *code += &format!("pub const ENTITY_POSES: &[&str] = &{poses:?};\n");
    // The two independently generated inventories must agree, including exact numeric IDs.
    *code += "#[derive(Clone, Copy, Debug)]\npub struct EntitySpec { pub update_interval: u32, pub dimensions: [f32; 3] }\npub const ENTITY_SPECS: &[EntitySpec] = &[\n";
    let types = facts["types"].as_array().unwrap();
    assert_eq!(
        types.len() as u64,
        target["entity_schema"]["count"].as_u64().unwrap()
    );
    for (id, entry) in types.iter().enumerate() {
        assert_eq!(entry["id"].as_u64().unwrap() as usize, id);
        let interval = entry["update_interval"].as_u64().unwrap();
        assert!(interval > 0 && interval <= u32::MAX as u64);
        let dim = entry["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect::<Vec<_>>();
        assert!(dim.len() == 3 && dim.iter().all(|n| n.is_finite() && *n >= 0.));
        *code += &format!("EntitySpec {{ update_interval: {interval}, dimensions: {dim:?} }},\n");
        assert_eq!(entry["name"].as_str().unwrap(), entity_names[id]);
    }
    *code += "];\n";
}
