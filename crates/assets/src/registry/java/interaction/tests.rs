use super::*;

fn fixture() -> Vec<u8> {
    // One static state, hardness .5, tool not required, half-height outline, no override.
    let mut bytes = b"CBJHITS1\0\0\0\x01\0\0\0\0\0\x3f\0\0\0\0\0\0\0\x01".to_vec();
    bytes.extend([0; 24]);
    bytes.extend([0x3f, 0xf0, 0, 0, 0, 0, 0, 0]);
    bytes.extend([0x3f, 0xe0, 0, 0, 0, 0, 0, 0]);
    bytes.extend([0x3f, 0xf0, 0, 0, 0, 0, 0, 0]);
    bytes.extend([0; 4]);
    bytes
}
fn load(bytes: &[u8]) -> Result<Table, Error> {
    Table::load(bytes, &format!("{:x}", Sha256::digest(bytes)), 1)
}
#[test]
fn independent_shape_vector_and_every_truncation() {
    let bytes = fixture();
    let table = load(&bytes).unwrap();
    let state = table.state(0).unwrap();
    assert_eq!(state.destroy_speed, 0.5);
    assert!(!state.requires_tool);
    let Shape::Static {
        outline,
        interaction,
    } = &state.shape
    else {
        panic!("static shape")
    };
    assert_eq!(
        &**outline,
        &[Bounds {
            min: [0.; 3],
            max: [1., 0.5, 1.]
        }]
    );
    assert!(interaction.is_empty());
    assert!(table.state(1).is_none());
    for end in 0..bytes.len() {
        assert!(load(&bytes[..end]).is_err(), "{end}");
    }
    assert!(Table::load(&bytes, "bad", 1).is_err());
    assert!(Table::load(&bytes, &format!("{:x}", Sha256::digest(&bytes)), 2).is_err());
}
#[test]
fn rejects_invalid_boolean_bounds_context_and_trailing_data() {
    let good = fixture();
    for (offset, value) in [(16, 2), (16, 1), (22, 255), (26, 0x40)] {
        let mut bytes = good.clone();
        bytes[offset] = value;
        assert!(load(&bytes).is_err(), "{offset}/{value}");
    }
    let mut nan = good.clone();
    nan[26] = 0x7f;
    nan[27] = 0xf8;
    assert!(load(&nan).is_err());
    let mut trailing = good;
    trailing.push(0);
    assert!(load(&trailing).is_err());
}
