//! Generates the pinned native attribute registry from the maintained codec reference.
use sha2::{Digest, Sha256};

pub fn generate(code: &mut String, target: &serde_json::Value) {
    let schema = &target["attribute_schema"];
    let path = schema["path"].as_str().unwrap();
    println!("cargo:rerun-if-changed={path}");
    let source = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&source)),
        schema["sha256"].as_str().unwrap(),
        "attribute reference hash drift"
    );
    let source = String::from_utf8(source).unwrap();
    *code += "#[derive(Clone, Copy, Debug)]\npub struct AttributeSpec { pub name: &'static str, pub minimum: f64, pub maximum: f64 }\npub const ATTRIBUTES: &[AttributeSpec] = &[\n";
    let mut count = 0;
    for line in source.lines().map(str::trim) {
        let Some((name, arguments)) = line.split_once('(') else {
            continue;
        };
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') {
            continue;
        }
        let arguments = arguments
            .strip_suffix("),")
            .or_else(|| arguments.strip_suffix(");"))
            .expect("pinned attribute declaration");
        let fields = arguments.split(',').map(str::trim).collect::<Vec<_>>();
        assert_eq!(fields.len(), 4, "pinned attribute fields");
        let identifier: String = serde_json::from_str(fields[0]).unwrap();
        let values = fields[1..]
            .iter()
            .map(|s| s.parse::<f64>().unwrap())
            .collect::<Vec<_>>();
        assert!(
            values.iter().all(|v| v.is_finite())
                && values[1] <= values[0]
                && values[0] <= values[2]
        );
        *code += &format!(
            "AttributeSpec {{ name: {identifier:?}, minimum: {:?}, maximum: {:?} }},\n",
            values[1], values[2]
        );
        count += 1;
    }
    assert_eq!(
        count,
        schema["count"].as_u64().unwrap(),
        "pinned attribute inventory"
    );
    *code += "];\n";
}
