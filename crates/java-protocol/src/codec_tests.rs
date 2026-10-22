use super::*;

#[test]
fn signed_varints_match_wire_vectors() {
    for (value, wire) in [
        (0, vec![0]),
        (127, vec![0x7f]),
        (128, vec![0x80, 1]),
        (255, vec![0xff, 1]),
        (i32::MAX, vec![0xff, 0xff, 0xff, 0xff, 7]),
        (i32::MIN, vec![0x80, 0x80, 0x80, 0x80, 8]),
        (-1, vec![0xff, 0xff, 0xff, 0xff, 0x0f]),
    ] {
        let mut encoded = Vec::new();
        write_varint(value, &mut encoded);
        assert_eq!(encoded, wire);
        let mut input = wire.as_slice();
        assert_eq!(read_varint(&mut input).unwrap(), value);
        assert!(input.is_empty());
    }
}

#[test]
fn malformed_varints_do_not_wrap_or_consume_unbounded_data() {
    for wire in [&[0x80][..], &[0x80, 0x80, 0x80, 0x80, 0x10], &[0xff; 6]] {
        let mut input = wire;
        assert!(read_varint(&mut input).is_err());
    }
    // Java permits a non-minimal but bounded encoding.
    assert_eq!(read_varint(&mut &[0x80, 0][..]).unwrap(), 0);
}

#[test]
fn strings_count_utf16_units_not_utf8_bytes_or_unicode_scalars() {
    let mut wire = Vec::new();
    write_string("😀", 2, &mut wire).unwrap();
    assert_eq!(wire, [4, 0xf0, 0x9f, 0x98, 0x80]);
    assert_eq!(read_string(&mut &wire[..], 2).unwrap(), "😀");
    assert!(read_string(&mut &wire[..], 1).is_err());
    assert!(write_string("😀", 1, &mut Vec::new()).is_err());
    assert!(read_string(&mut &[2, b'a'][..], 2).is_err());
    assert!(read_string(&mut &[1, 0xff][..], 2).is_err());
    assert!(read_string(&mut &[0xff, 0xff, 0xff, 0xff, 0x0f][..], 2).is_err());
}

#[test]
fn frame_bounds_include_the_packet_id() {
    assert_eq!(frame(&[0]).unwrap(), [1, 0]);
    assert!(frame(&[]).is_err());
    let maximum = vec![0; MAX_FRAME_BYTES];
    let encoded = frame(&maximum).unwrap();
    assert_eq!(&encoded[..3], &[0xff, 0xff, 0x7f]);
    assert!(frame(&vec![0; MAX_FRAME_BYTES + 1]).is_err());
}
