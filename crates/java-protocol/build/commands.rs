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
