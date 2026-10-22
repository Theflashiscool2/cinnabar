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
    let mut specs = Vec::new();
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
        specs.push((identifier, values[1], values[2]));
        count += 1;
    }
    assert_eq!(
        count,
        schema["count"].as_u64().unwrap(),
        "pinned attribute inventory"
    );
    *code += "];\n";
    let report = &target["player_attributes_report"];
    let path = report["path"].as_str().unwrap();
    println!("cargo:rerun-if-changed={path}");
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(bytes.len() as u64, report["bytes"].as_u64().unwrap());
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        report["sha256"].as_str().unwrap(),
        "native player attribute hash drift"
    );
    let values: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let values = values.as_array().unwrap();
    assert_eq!(values.len() as u64, report["count"].as_u64().unwrap());
    *code += "pub const PLAYER_ATTRIBUTES: &[(usize, f64)] = &[\n";
    let mut previous = None;
    for value in values {
        let id = value["id"].as_u64().unwrap() as usize;
        assert!(
            previous.is_none_or(|old| old < id),
            "native player attribute order"
        );
        previous = Some(id);
        let (name, min, max) = &specs[id];
        assert_eq!(name, value["name"].as_str().unwrap());
        assert_eq!(*min, value["minimum"].as_f64().unwrap());
        assert_eq!(*max, value["maximum"].as_f64().unwrap());
        let base = value["base"].as_f64().unwrap();
        assert!(base.is_finite() && base >= *min && base <= *max);
        *code += &format!("({id}, {base:?}),\n");
    }
    *code += "];\n";
}
