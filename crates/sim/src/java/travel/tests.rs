use super::*;
mod terrain;

#[test]
fn input_rotation_matches_independent_vanilla_vectors() {
    // Obtained by invoking Entity.getInputVector from the hash-pinned server archive.
    for (input, speed, yaw, bits) in [
        ([0., 0., 0.98], 0.1, 0., [0, 0, 0x3fb916873147ae14]),
        (
            [0.7, 0., 0.7],
            0.13,
            137.25,
            [0xbfc075c8d6c44719, 0, 0xbf74aa276c1d8338],
        ),
        (
            [1., 0., 1.],
            0.02,
            -75.125,
            [0x3f91b66a8934f8b0, 0, 0xbf848f49d747218e],
        ),
        ([0.; 3], 0.1, 90., [0; 3]),
    ] {
        assert_eq!(input_vector(input, speed, yaw).map(f64::to_bits), bits);
    }
}

#[test]
fn body_dimensions_keep_vanilla_float_widening_and_crouch_clearance() {
    let standing = bounds([16_777_216., 64., 0.], false, 1.);
    assert_eq!(standing.max[1], 64. + f64::from(1.8_f32));
    assert_eq!(standing.min[0], 16_777_216. - f64::from(0.3_f32));
    let crouched = bounds([0., 64., 0.], true, 1.);
    assert_eq!(crouched.max[1], 65.5);
    assert_eq!(Body::default().eye_height(1.), f64::from(1.62_f32));
}

#[test]
fn sneak_edge_protection_stops_before_losing_support() {
    let shape = bounds([0.5, 0., 0.5], true, 1.);
    let floor = Bounds {
        min: [0., -1., 0.],
        max: [1., 0., 1.],
    };
    let body = Body {
        on_ground: true,
        ..Default::default()
    };
    let result = backed_off(shape, [1., -0.08, 0.], body, 0.6, &[floor]);
    assert!((result[0] - 0.75).abs() < 1.0e-12);
    assert_eq!([result[1], result[2]], [-0.08, 0.]);
    assert_eq!(
        backed_off(shape, [1., -0.08, 0.], body, 0.6, &[]),
        [0., -0.08, 0.]
    );
}

#[test]
fn diagonal_input_preserves_unit_bound_and_sneak_speed() {
    assert_eq!(modified_input([0., 1.], 1.), [0., 0.98]);
    let diagonal = modified_input([1., 1.], 1.);
    assert!((diagonal[0] * diagonal[0] + diagonal[1] * diagonal[1] - 1.).abs() < 1.0e-6);
    assert!(modified_input([0., 1.], 0.3)[1] < 0.3);
    assert_eq!(modified_input([0., 1.], 0.), [0.; 2]);
}
