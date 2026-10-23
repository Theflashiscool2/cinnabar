use super::*;

#[test]
fn native_stepped_interpolation_matches_pinned_runtime_vectors() {
    // Invoked the pinned vanilla InterpolationData directly with an original reflection helper.
    // Numeric results are independent of this implementation's encoder and path admission.
    let mut track = Track::new(
        Pose {
            yaw: 170.,
            ..Default::default()
        },
        3,
    )
    .unwrap();
    // The independent probe starts after a completed prior target at the initial angle.
    track.retarget(track.current, &[]).unwrap();
    for _ in 0..4 {
        track.tick();
    }
    track
        .retarget(
            Pose {
                position: [1., 0., 3.],
                yaw: -170.,
                pitch: 30.,
            },
            &[
                PositionStep {
                    position: [1., 0., 0.],
                    tick_offset: 2,
                },
                PositionStep {
                    position: [1., 0., 3.],
                    tick_offset: 3,
                },
            ],
        )
        .unwrap();
    for (position, yaw, pitch) in [
        ([0.5, 0., 0.], 0x432e0000, 0x40c00000),
        ([1., 0., 0.22222233563661575], 0x4332e38e, 0x41555556),
        ([1., 0., 1.4567902386188507], 0x4337d3c1, 0x41a5ed0a),
        ([1., 0., 2.613168776035309], 0x433c73e2, 0x41dd6e9e),
        ([1., 0., 3.], 0xc32a0000, 0x41f00000),
    ] {
        track.tick();
        assert_eq!(track.current.position, position);
        assert_eq!(track.current.yaw.to_bits(), yaw);
        assert_eq!(track.current.pitch.to_bits(), pitch);
    }
    assert_eq!(track.pending_steps(), 0);
}

#[test]
fn completed_native_rotation_is_normalized_independently_of_the_raw_target() {
    let mut track = Track::new(Pose::default(), 3).unwrap();
    track
        .retarget(
            Pose {
                yaw: 720.,
                pitch: 400.,
                ..Default::default()
            },
            &[],
        )
        .unwrap();
    for _ in 0..4 {
        track.tick();
    }
    // Pinned Entity.setRot wraps both components after the handler samples its raw target.
    assert_eq!(track.current.yaw, 0.);
    assert_eq!(track.current.pitch, 40.);
    assert_eq!(track.pending_steps(), 0);
}

#[test]
fn native_interpolation_rejects_invalid_spans_before_allocating_a_path() {
    let mut track = Track::new(
        Pose {
            position: [f64::MAX, 0., 0.],
            ..Default::default()
        },
        3,
    )
    .unwrap();
    assert!(
        track
            .retarget(
                Pose {
                    position: [-f64::MAX, 0., 0.],
                    ..Default::default()
                },
                &[]
            )
            .is_err()
    );
    assert_eq!(track.pending_steps(), 0);
    let mut track = Track::new(Pose::default(), 3).unwrap();
    assert!(
        track
            .retarget(
                Pose {
                    position: [1.; 3],
                    ..Default::default()
                },
                &[
                    PositionStep {
                        position: [0.5; 3],
                        tick_offset: i32::MAX
                    },
                    PositionStep {
                        position: [1.; 3],
                        tick_offset: 1
                    },
                ]
            )
            .is_err()
    );
    assert_eq!(track.pending_steps(), 0);
    let path = vec![
        PositionStep {
            position: [1.; 3],
            tick_offset: 1
        };
        MAX_PENDING_STEPS + 1
    ];
    assert!(
        track
            .retarget(
                Pose {
                    position: [1.; 3],
                    ..Default::default()
                },
                &path
            )
            .is_err()
    );
    assert_eq!(track.pending_steps(), 0);
}

#[test]
fn native_interpolation_preserves_coalesced_turns_and_snaps_retire_paths() {
    let mut track = Track::new(Pose::default(), 3).unwrap();
    track
        .retarget(
            Pose {
                position: [3., 0., 0.],
                ..Default::default()
            },
            &[],
        )
        .unwrap();
    track
        .retarget(
            Pose {
                position: [3., 0., 3.],
                yaw: 90.,
                ..Default::default()
            },
            &[],
        )
        .unwrap();
    assert_eq!(track.pending_steps(), 2);
    track.tick();
    assert_eq!(track.current.position, [1.0000000298023224, 0., 0.]);
    let snap = Pose {
        position: [-30., 5., -20.],
        yaw: -90.,
        pitch: 20.,
    };
    track.snap(snap);
    track.tick();
    assert_eq!(track.current, snap);
    assert_eq!(track.sample(0.5), snap);
    assert_eq!(track.pending_steps(), 0);
    let before = track.current;
    assert!(
        track
            .retarget(
                Pose::default(),
                &[PositionStep {
                    position: [0.; 3],
                    tick_offset: 0
                }]
            )
            .is_err()
    );
    assert_eq!(track.current, before);
    assert_eq!(track.pending_steps(), 0);
}
