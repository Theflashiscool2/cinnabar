//! Derives the target's game-mode and notification IDs from maintained pinned enums.
use sha2::{Digest, Sha256};

pub fn generate(code: &mut String, target: &serde_json::Value) {
    for (key, symbol) in [("game_modes", "GAME_MODE"), ("game_events", "GAME_EVENT")] {
        let pin = &target["hud_schema"][key];
        let path = pin["path"].as_str().unwrap();
        println!("cargo:rerun-if-changed={path}");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            pin["sha256"].as_str().unwrap()
        );
        let source = String::from_utf8(bytes).unwrap();
        let body = source
            .split_once("public enum ")
            .unwrap()
            .1
            .split_once('{')
            .unwrap()
            .1
            .split_once(';')
            .unwrap()
            .0;
        let names = body.split(',').map(str::trim).collect::<Vec<_>>();
        assert_eq!(names.len() as u64, pin["count"].as_u64().unwrap());
        assert_eq!(
            names
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            names.len()
        );
        for (id, name) in names.iter().enumerate() {
            assert!(!name.is_empty() && name.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'));
            *code += &format!("pub const {symbol}_{name}: u8 = {id};\n");
        }
        *code += &format!("pub const {symbol}_NAMES: &[&str] = &{names:?};\n");
    }
}
