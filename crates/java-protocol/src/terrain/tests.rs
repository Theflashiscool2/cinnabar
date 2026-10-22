use super::*;
#[test]
fn singleton_has_no_long_count_and_preserves_next_field() {
    let mut r = Reader(&[0, 42, 99]);
    let p = Palette::read(&mut r, 4096, 8).unwrap();
    assert_eq!(p.get(4095), Some(42));
    assert_eq!(r.byte().unwrap(), 99);
    assert_eq!(p.get(4096), None);
}
#[test]
fn five_bit_palette_padding_and_yzx_order() {
    let mut bytes = vec![5, 3, 10, 20, 30];
    // Twelve entries per long. Entry 11 = 1; entry 12 = 2. Padding is ignored.
    bytes.extend(((1u64 << 55) | (15 << 60)).to_be_bytes());
    bytes.extend(2u64.to_be_bytes());
    let p = Palette::read(&mut Reader(&bytes), 24, 8).unwrap();
    assert_eq!(p.get(10), Some(10));
    assert_eq!(p.get(11), Some(20));
    assert_eq!(p.get(12), Some(30));
    assert_eq!(p.get(13), Some(10));
}
#[test]
fn direct_palette_has_no_palette_or_word_count() {
    let bytes = [9, 0, 0, 0, 0, 0, 0, 3, 255]; // two 9-bit values: 511, 1.
    let p = Palette::read(&mut Reader(&bytes), 2, 8).unwrap();
    assert_eq!(p.get(0), Some(511));
    assert_eq!(p.get(1), Some(1));
    assert!(p.validate_registry(511).is_err());
    assert!(p.validate_registry(512).is_ok());
}
#[test]
fn malformed_palettes_and_truncation_fail() {
    for bytes in [
        &[32u8][..],
        &[1, 0],
        &[1, 1, 0],
        &[0, 255, 255, 255, 255, 15],
    ] {
        assert!(Palette::read(&mut Reader(bytes), 64, 3).is_err());
    }
    let mut bytes = vec![1, 1, 7];
    bytes.extend(1u64.to_be_bytes()); // index 1 but only index 0 exists.
    assert!(Palette::read(&mut Reader(&bytes), 64, 3).is_err());
}
#[test]
fn light_masks_are_byte_arrays_with_boundary_sections() {
    let mut bytes = vec![1, 2, 0, 1, 5, 1, 7, 1, 128, 16];
    bytes.extend([0xa3; 2048]);
    bytes.push(0);
    let light = Light::read(&mut Reader(&bytes), 1).unwrap();
    assert_eq!(light.sky[0], Some(vec![]));
    assert_eq!(light.sky[1].as_ref().unwrap()[0], 0xa3);
    assert_eq!(light.sky[2], Some(vec![]));
    assert!(light.block.iter().all(|v| v == &Some(vec![])));
    assert!(Light::read(&mut Reader(&[1, 8, 0, 0, 0, 0, 0]), 1).is_err());
    assert!(Light::read(&mut Reader(&[1, 1, 0, 1, 1, 0, 0, 0]), 1).is_err());
    assert!(Light::read(&mut Reader(&[1, 1, 0, 0, 0, 0, 0]), 1).is_err());
}
#[test]
fn signed_block_section_and_chunk_positions() {
    assert_eq!(block_position(-1), [-1, -1, -1]);
    let mut bytes = vec![255; 8];
    bytes.extend([1, 0xa0, 0x62]); // state 3, local x=1,z=2,y=0.
    let u = section_updates(&mut Reader(&bytes)).unwrap();
    assert_eq!(u[0].position, [-15, -16, -14]);
    assert_eq!(u[0].state, 3);
    let c = Context::new(-64, 384, 1).unwrap();
    let bytes = [255, 255, 255, 254, 255, 255, 255, 253];
    assert!(matches!(
        c.decode("ClientboundForgetLevelChunkPacket", &mut Reader(&bytes))
            .unwrap(),
        Some(Packet::Unload { x: -3, z: -2 })
    ));
}
#[test]
fn varlong_overflow_and_truncation_are_rejected() {
    assert_eq!(
        Reader(&[255, 255, 255, 255, 255, 255, 255, 255, 255, 1])
            .varlong()
            .unwrap(),
        u64::MAX
    );
    assert!(Reader(&[255; 10]).varlong().is_err());
    assert!(Reader(&[128]).varlong().is_err());
}
#[test]
fn full_chunk_has_fluid_count_and_exact_section_boundary() {
    let c = Context::new(-64, 16, 1).unwrap();
    // x=-1,z=2; zero heightmaps; eight section bytes; no entities, no light updates.
    let bytes = [
        255, 255, 255, 255, 0, 0, 0, 2, 0, 8, 0, 1, 0, 1, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut r = Reader(&bytes);
    let p = c.chunk(&mut r).unwrap();
    r.finish().unwrap();
    assert_eq!((p.x, p.z), (-1, 2));
    assert_eq!(p.sections[0].fluid_count, 1);
    assert_eq!(p.sections[0].blocks.get(4095), Some(7));
    for end in 0..bytes.len() {
        assert!(c.chunk(&mut Reader(&bytes[..end])).is_err(), "prefix {end}");
    }
}

#[test]
fn heightmap_values_must_fit_the_dimension() {
    let mut bytes = vec![0; 8];
    bytes.extend([1, 1, 22]);
    bytes.extend([255; 22 * 8]);
    assert!(matches!(
        Context::new(0, 16, 1).unwrap().chunk(&mut Reader(&bytes)),
        Err(Error::Invalid("Java heightmap value outside dimension"))
    ));
}
