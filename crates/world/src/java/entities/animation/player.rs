//! Standing, unarmed player body turning and humanoid gait from the pinned native classes.
use super::{Presentation, Sample, wrap};
use crate::java::player::trig;

pub(super) fn body(presentation: &mut Presentation, dx: f64, dz: f64) {
    let yaw = presentation.track.current.yaw;
    let mut target = presentation.body;
    if (dx * dx + dz * dz) as f32 > 0.0025000002 {
        let direction = dz.atan2(dx) as f32 * (180. / std::f32::consts::PI) - 90.;
        let difference = (wrap(yaw) - direction).abs();
        target = if difference > 95. && difference < 265. {
            direction - 180.
        } else {
            direction
        };
    }
    turn(presentation, target);
}

fn turn(presentation: &mut Presentation, target: f32) {
    presentation.body += wrap(target - presentation.body) * 0.3;
    let difference = wrap(presentation.track.current.yaw - presentation.body);
    // Player narrows this limit while blocking; that item-use pose is not in this slice.
    if difference.abs() > 50. {
        presentation.body += difference - difference.signum() * 50.;
    }
}

impl Sample {
    /// Native empty-hand standing pose: head, right/left arm, right/left leg.
    /// Crouching, item-use, attack, swimming and passenger poses require separate authority.
    pub fn player_rotations(self) -> [[f32; 3]; 5] {
        let phase = self.walk_position * 0.6662;
        let cosine = |a| trig(a)[1] as f32;
        let a = cosine(phase) * self.walk_speed;
        let b = cosine(phase + std::f32::consts::PI) * self.walk_speed;
        let bob_x = trig(self.age_in_ticks * 0.067)[0] as f32 * 0.05;
        let bob_z = cosine(self.age_in_ticks * 0.09) * 0.05 + 0.05;
        [
            [
                self.pose.pitch.to_radians(),
                wrap(self.head_yaw - self.body_yaw).to_radians(),
                0.,
            ],
            [b + bob_x, 0., bob_z],
            [a - bob_x, 0., -bob_z],
            [cosine(phase) * 1.4 * self.walk_speed, 0.005, 0.005],
            [
                cosine(phase + std::f32::consts::PI) * 1.4 * self.walk_speed,
                -0.005,
                -0.005,
            ],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::java::entities::{animation::BodyKind, interpolation::Pose};
    #[test]
    fn player_body_turn_matches_unarmed_native_probe() {
        let mut player = Presentation::new(
            Pose {
                yaw: 90.,
                ..Default::default()
            },
            0.,
            2,
            BodyKind::Player,
        )
        .unwrap();
        for expected in [40., 55., 65.5, 72.85] {
            turn(&mut player, 90.);
            assert!((player.body - expected).abs() < 0.00001);
        }
        player.body = 0.;
        player.track.current.yaw = -45.;
        body(&mut player, 1., 0.25);
        // Native Mth.atan2 is table-assisted; this sample qualifies the Rust atan2 path only
        // to the stated tolerance, not universal bit-for-bit animation parity.
        assert!((f64::from(player.body) - -22.789131).abs() < 0.0001);
    }
    #[test]
    fn empty_hand_model_pose_matches_original_runtime_vectors() {
        for (position, speed, age, right_arm_x, right_arm_z, right_leg_x) in [
            (0., 0., 0., 0., 0.100000001, 0.),
            (2.2, 0.4, 20.5, 0.00703798234, 0.0364612006, 0.0587886684),
            (5.5, 0.8, 80.25, 0.653925657, 0.0795186460, -0.970609963),
        ] {
            let sample = Sample {
                pose: Pose {
                    pitch: 15.,
                    ..Default::default()
                },
                body_yaw: 0.,
                head_yaw: -25.,
                walk_position: position,
                walk_speed: speed,
                age_in_ticks: age,
            };
            let expected: [[f64; 3]; 5] = [
                [0.261799395, -0.436332315, 0.],
                [right_arm_x, 0., right_arm_z],
                [-right_arm_x, 0., -right_arm_z],
                [right_leg_x, 0.005, 0.005],
                [-right_leg_x, -0.005, -0.005],
            ];
            for (actual, expected) in sample
                .player_rotations()
                .into_iter()
                .flatten()
                .zip(expected.into_iter().flatten())
            {
                assert!(
                    (f64::from(actual) - expected).abs() < 0.000001,
                    "{actual} != {expected}"
                );
            }
        }
    }
}
