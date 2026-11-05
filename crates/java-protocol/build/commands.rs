//! Pins native command parser identities and network-property shapes to the verified report.
use sha2::{Digest, Sha256};
pub fn generate(code: &mut String, target: &serde_json::Value) {
    let pin = &target["command_argument_schema"];
    let path = pin["path"].as_str().unwrap();
    println!("cargo:rerun-if-changed={path}");
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        pin["sha256"].as_str().unwrap()
    );
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    let mut enums = entries
        .iter()
        .filter_map(|e| e.get("modes").and_then(serde_json::Value::as_array));
    let modes = enums.next().expect("native string modes");
    assert!(enums.next().is_none(), "ambiguous native mode enum");
    code.push_str("pub mod command_string_mode {\n");
    for (id, name) in modes.iter().enumerate() {
        let name = name.as_str().unwrap();
        assert!(!name.is_empty() && name.bytes().all(|c| c.is_ascii_uppercase() || c == b'_'));
        code.push_str(&format!("pub const {name}:i32={id};\n"));
    }
    let ids: Vec<_> = (0..modes.len() as i32).collect();
    code.push_str(&format!("pub const ALL:&[i32]=&{ids:?};\n}}\n"));
    code.push_str("pub const COMMAND_ARGUMENTS:&[(&str,&str,bool)]=&[\n");
    for (id, entry) in entries.iter().enumerate() {
        assert_eq!(entry["id"].as_u64().unwrap(), id as u64);
        code.push_str(&format!(
            "({:?},{:?},{}),\n",
            entry["name"].as_str().unwrap(),
            entry["properties"].as_str().unwrap(),
            entry["signable"].as_bool().unwrap()
        ));
    }
    code.push_str("];\n");
}
