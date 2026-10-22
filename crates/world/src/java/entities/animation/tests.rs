use super::*;

#[test]
fn native_body_control_matches_independent_stationary_and_moving_vectors() {
    let mut mob = Mob::new(Pose::default(), 100., 3).unwrap();
    mob.body = 0.;
    for expected in [
        25., 25., 25., 25., 25., 25., 25., 25., 25., 25., 25., 32.5, 40., 47.5, 55., 62.5, 70.,
        77.5, 85., 92.5, 100., 100.,
    ] {
        mob.tick(false, true, false, false);
        assert_eq!(mob.sample(1.).body_yaw, expected);
        assert_eq!(mob.sample(1.).head_yaw, 100.);
    }
    mob.retarget(
        Pose {
            position: [0.3, 0., 0.],
            yaw: -150.,
            pitch: 0.,
        },
        &[],
    )
    .unwrap();
    mob.tick(false, true, false, false);
    // The movement is interpolated: body follows that frame's yaw, head stays within 75°.
    assert_eq!(mob.body, mob.track.current.yaw);
    assert!(wrap(mob.head - mob.body).abs() <= 75.);
}

#[test]
fn native_walk_samples_match_pinned_runtime_and_model_trigonometry() {
    let mut walk = Walk::default();
    // Independent WalkAnimationState/Mth results from the pinned vanilla runtime.
    for (distance, position, speed, leg) in [
        (0.1, 0x3da3d70b, 0x3da3d70b, 0x3de50d27),
        (0.2, 0x3ebc6a80, 0x3e9374bc, 0x3ec84420),
        (0.4, 0x3f669ad4, 0x3f086595, 0x3f1d982b),
        (0., 0x3fb5d24a, 0x3f0509c0, 0x3ed9cee5),
        (0., 0x3fddbb9d, 0x3e9fa54d, 0x3e34f33f),
    ] {
        walk.update(distance, false);
        let sample = Sample {
            pose: Pose::default(),
            body_yaw: 0.,
            head_yaw: 0.,
            walk_position: (walk.position - walk.speed * 0.5) * walk.scale,
            walk_speed: (walk.old_speed + (walk.speed - walk.old_speed) * 0.5).min(1.),
        };
        assert_eq!(sample.walk_position.to_bits(), position);
        assert_eq!(sample.walk_speed.to_bits(), speed);
        let angles = sample.rotations();
        assert_eq!(angles[1][0].to_bits(), leg);
        assert_eq!(angles[2][0].to_bits(), leg | 0x80000000);
        assert_eq!(angles[1], angles[4]);
        assert_eq!(angles[2], angles[3]);
    }
}

#[test]
fn native_head_body_and_gait_retire_with_a_discontinuity() {
    let mut mob = Mob::new(Pose::default(), 0., 3).unwrap();
    mob.head_to(90.);
    for expected in [30., 60., 90.] {
        mob.tick(false, true, false, false);
        assert_eq!(mob.sample(1.).head_yaw, expected);
    }
    mob.retarget(
        Pose {
            position: [3., 0., 0.],
            yaw: 90.,
            ..Default::default()
        },
        &[],
    )
    .unwrap();
    mob.tick(true, true, false, false);
    assert!(mob.sample(1.).walk_speed > 0.);
    // One of three interpolation ticks has elapsed; body follows the current 30° pose.
    assert_eq!(mob.sample(1.).body_yaw, 30.);
    let corrected = Pose {
        position: [-90., 5., 20.],
        yaw: 20.,
        pitch: 30.,
    };
    mob.snap(corrected);
    assert_eq!(mob.sample(0.5).pose, corrected);
    assert_eq!(mob.sample(0.5).walk_speed, 0.);
    assert_eq!(mob.track.pending_steps(), 0);
}
