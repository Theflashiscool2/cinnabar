//! Derives native entity registry names from the target-pinned maintained schema.
use sha2::{Digest, Sha256};

pub fn generate(code: &mut String, target: &serde_json::Value) {
    let pin = &target["entity_schema"];
    let path = pin["path"].as_str().unwrap();
    println!("cargo:rerun-if-changed={path}");
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        pin["sha256"].as_str().unwrap(),
        "native entity registry hash drift"
    );
    let source = String::from_utf8(bytes).unwrap();
    let body = source.split_once("public enum EntityType {").unwrap().1;
    let body = body.split_once(';').unwrap().0;
    let names: Vec<_> = body
        .split(',')
        .map(str::trim)
        .map(|entry| entry.strip_suffix("(true)").unwrap_or(entry))
        .map(|name| {
            assert!(!name.is_empty());
            assert!(name.bytes().all(|c| c.is_ascii_uppercase() || c == b'_'));
            format!("minecraft:{}", name.to_ascii_lowercase())
        })
        .collect();
    assert_eq!(names.len() as u64, pin["count"].as_u64().unwrap());
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len());
    *code += &format!("pub const ENTITY_TYPES: &[&str] = &{names:?};\n");
}
