//! Independently assembled, pinned-version component payloads shared by batch wire fixtures.
pub fn fixtures() -> Vec<(String, Vec<u8>)> {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/item-components-777.json")).unwrap();
    data["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let hex = row["hex"].as_str().unwrap();
            assert_eq!(hex.len() % 2, 0);
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            (row["component"].as_str().unwrap().to_owned(), bytes)
        })
        .collect()
}
pub fn item(name: &str, payload: &[u8]) -> Vec<u8> {
    let id = java_protocol::target::ITEM_COMPONENTS
        .iter()
        .position(|c| c.name == name)
        .unwrap();
    let mut bytes = vec![1, 55, 1, 0];
    java_protocol::codec::write_varint(id as i32, &mut bytes);
    bytes.extend_from_slice(payload);
    bytes
}
