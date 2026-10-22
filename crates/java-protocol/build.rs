use std::{collections::BTreeMap, env, fs, path::PathBuf};
#[path = "build/attributes.rs"]
mod attributes;
#[path = "build/entities.rs"]
mod entities;
#[path = "build/metadata.rs"]
mod metadata;

fn main() {
    println!("cargo:rerun-if-changed=data/target.json");
    println!("cargo:rerun-if-changed=data/MinecraftCodec.java");
    let target: serde_json::Value =
        serde_json::from_str(&fs::read_to_string("data/target.json").unwrap()).unwrap();
    let source = fs::read_to_string("data/MinecraftCodec.java").unwrap();
    let version = target["game_version"].as_str().unwrap();
    let protocol = target["protocol"].as_i64().unwrap();
    assert!(
        source.contains(&format!(".protocolVersion({protocol})")),
        "codec/manifest protocol drift"
    );
    assert!(
        source.contains(&format!(".minecraftVersion(\"{version}\")")),
        "codec/manifest release drift"
    );
    let mut tables: BTreeMap<String, BTreeMap<&str, Vec<String>>> = BTreeMap::new();
    let mut state = String::new();
    for line in source.lines().map(str::trim) {
        if let Some(rest) = line
            .trim_start_matches(')')
            .strip_prefix(".state(ProtocolState.")
        {
            state = rest.split(',').next().unwrap().to_ascii_lowercase();
            assert!(
                tables
                    .insert(
                        state.clone(),
                        BTreeMap::from([("clientbound", Vec::new()), ("serverbound", Vec::new())])
                    )
                    .is_none()
            );
        }
        for (prefix, direction) in [
            (".registerClientboundPacket(", "clientbound"),
            (".registerServerboundPacket(", "serverbound"),
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let name = rest
                    .split_once(".class,")
                    .expect("pinned codec registration shape")
                    .0;
                let packets = tables.get_mut(&state).unwrap().get_mut(direction).unwrap();
                assert!(!packets.iter().any(|p| p == name), "duplicate packet");
                packets.push(name.to_owned());
            }
        }
    }
    assert_eq!(tables.len(), 5, "pinned codec state inventory changed");
    let mut code =
        format!("pub const GAME_VERSION:&str={version:?};\npub const PROTOCOL:i32={protocol};\n");
    attributes::generate(&mut code, &target);
    let entity_names = entities::generate(&mut code, &target);
    metadata::generate(&mut code, &target, &entity_names);
    let report = &target["block_registry_report"];
    code += &format!(
        "pub const BLOCK_REPORT_SHA256:&str={:?};\npub const BLOCK_REPORT_BYTES:usize={};\npub const BLOCK_STATE_COUNT:usize={};\n",
        report["sha256"].as_str().unwrap(),
        report["bytes"].as_u64().unwrap(),
        report["states"].as_u64().unwrap()
    );
    let client = &target["client_assets"];
    let physics = &target["physics_report"];
    code += &format!(
        "pub const PHYSICS_REPORT_SHA256:&str={:?};\npub const PHYSICS_REPORT_BYTES:u64={};\n",
        physics["sha256"].as_str().unwrap(),
        physics["bytes"].as_u64().unwrap()
    );
    code += &format!(
        "pub const CLIENT_ASSETS_SHA256:&str={:?};\npub const CLIENT_ASSETS_BYTES:u64={};\n",
        client["sha256"].as_str().unwrap(),
        client["bytes"].as_u64().unwrap()
    );
    for (state, directions) in tables {
        code += &format!("pub mod {state} {{\n");
        for (direction, packets) in directions {
            code += &format!("pub mod {direction} {{ pub const NAMES:&[&str]=&{packets:?}; }}\n");
        }
        code += "}\n";
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("target.rs"),
        code,
    )
    .unwrap();
}
