//! Standing/crouching, unarmed player body turning and humanoid gait from the pinned native classes.
use super::{Presentation, Sample, wrap};
use crate::java::player::trig;

#[cfg(test)]
pub(super) fn body(presentation: &mut Presentation, dx: f64, dz: f64) {
    body_with_swing(presentation, dx, dz, false);
}
pub(super) fn body_with_swing(presentation: &mut Presentation, dx: f64, dz: f64, swinging: bool) {
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
    turn(presentation, if swinging { yaw } else { target });
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
    /// Pinned HumanoidModel/AvatarRenderer posture, verified by ProbePlayerPosture.java.
    /// Crouching preserves gait/head rotation and changes local offsets, torso and arm pitch.
    pub fn player_posture(self, crouching: bool) -> super::PlayerPosture {
        self.player_posture_with_items(crouching, false, false)
    }
    /// Native idle ITEM arm pose is applied before crouch offsets and arm bobbing.
    /// Active-use and attack poses have their own authoritative animation lifecycle.
    pub fn player_posture_with_items(
        self,
        crouching: bool,
        right_item: bool,
        left_item: bool,
    ) -> super::PlayerPosture {
        let [head, mut right_arm, mut left_arm, right_leg, left_leg] = self.player_rotations();
        let bob_x = trig(self.age_in_ticks * 0.067)[0] as f32 * 0.05;
        if right_item {
            right_arm[0] = (right_arm[0] - bob_x) * 0.5 - std::f32::consts::PI / 10. + bob_x;
        }
        if left_item {
            left_arm[0] = (left_arm[0] + bob_x) * 0.5 - std::f32::consts::PI / 10. - bob_x;
        }
        let mut offsets = [
            [0., 0., 0.],
            [0., 0., 0.],
            [-5., 2., 0.],
            [5., 2., 0.],
            [-1.9, 12., 0.],
            [1.9, 12., 0.],
        ];
        if crouching {
            offsets[0][1] = 4.2;
            offsets[1][1] = 3.2;
            offsets[2][1] = 5.2;
            offsets[3][1] = 5.2;
            offsets[4][2] = 4.;
            offsets[5][2] = 4.;
            right_arm[0] += 0.4;
            left_arm[0] += 0.4;
        }
        super::PlayerPosture {
            rotations: [
                head,
                [if crouching { 0.5 } else { 0. }, 0., 0.],
                right_arm,
                left_arm,
                right_leg,
                left_leg,
            ],
            offsets,
            // AvatarRenderer.getRenderOffset: scale * -2 / 16 (unit-scale player).
            render_offset_y: if crouching { -0.125 } else { 0. },
        }
    }

    /// Native attack stage, preserving idle held items, crouch and final arm bobbing.
    pub fn player_posture_with_swing(
        self,
        crouching: bool,
        right_item: bool,
        left_item: bool,
        swing: Option<super::swing::Sample>,
        left_handed: bool,
    ) -> super::PlayerPosture {
        let mut p = self.player_posture_with_items(crouching, right_item, left_item);
        let Some(swing) = swing else {
            return p;
        };
        let bob_x = trig(self.age_in_ticks * 0.067)[0] as f32 * 0.05;
        let bob_z = trig(self.age_in_ticks * 0.09)[1] as f32 * 0.05 + 0.05;
        let crouch = if crouching { 0.4 } else { 0. };
        p.rotations[2][0] -= bob_x + crouch;
        p.rotations[3][0] -= -bob_x + crouch;
        p.rotations[2][2] -= bob_z;
        p.rotations[3][2] += bob_z;
        p.attack(swing, left_handed);
        p.rotations[2][0] += bob_x + crouch;
        p.rotations[3][0] += -bob_x + crouch;
        p.rotations[2][2] += bob_z;
        p.rotations[3][2] -= bob_z;
        p
    }

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
    #[test]
    fn crouching_keeps_native_gait_and_resets_without_stale_offsets() {
        let sample = Sample {
            pose: Pose {
                pitch: 15.,
                ..Default::default()
            },
            body_yaw: 0.,
            head_yaw: -25.,
            walk_position: 2.2,
            walk_speed: 0.4,
            age_in_ticks: 20.5,
        };
        let before = sample.player_posture(false);
        let crouch = sample.player_posture(true);
        // Independent Java 26.3 PlayerModel.setupAnim probe, wide AND slim.
        assert_eq!(
            crouch.offsets,
            [
                [0., 4.2, 0.],
                [0., 3.2, 0.],
                [-5., 5.2, 0.],
                [5., 5.2, 0.],
                [-1.9, 12., 4.],
                [1.9, 12., 4.],
            ]
        );
        for (actual, expected) in crouch.rotations.iter().flatten().zip([
            0.2617994,
            -0.43633232,
            0.,
            0.5,
            0.,
            0.,
            0.40703797,
            0.,
            0.0364612,
            0.39296204,
            0.,
            -0.0364612,
            0.05878867,
            0.005,
            0.005,
            -0.05878867,
            -0.005,
            -0.005,
        ]) {
            assert!((*actual - expected).abs() < 1e-6);
        }
        assert_eq!(crouch.render_offset_y, -2. / 16.);
        assert_eq!(sample.player_posture(false), before);
        assert_eq!(before.offsets[4], [-1.9, 12., 0.]);
        assert_eq!(before.render_offset_y, 0.);
    }

    #[test]
    fn native_player_postures_match_external_verified_runtime_probe() {
        let Some(path) = std::env::var_os("CINNABAR_JAVA_PLAYER_POSTURES") else {
            eprintln!(
                "SKIP native posture vectors: missing CINNABAR_JAVA_PLAYER_POSTURES from ProbePlayerPosture.java"
            );
            return;
        };
        let rows: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let rows = rows.as_array().unwrap();
        assert_eq!(
            rows.len(),
            12,
            "wide/slim, standing/crouching, three gait samples"
        );
        for row in rows {
            let samples = row["sample"].as_array().unwrap();
            let sample = Sample {
                pose: Pose {
                    pitch: 15.,
                    ..Default::default()
                },
                body_yaw: 0.,
                head_yaw: -25.,
                walk_position: samples[0].as_f64().unwrap() as f32,
                walk_speed: samples[1].as_f64().unwrap() as f32,
                age_in_ticks: samples[2].as_f64().unwrap() as f32,
            };
            let actual = sample.player_posture(row["crouching"].as_bool().unwrap());
            for (i, name) in super::super::PLAYER_PARTS.iter().enumerate() {
                let name = name.strip_prefix("root/").unwrap();
                for (field, values) in [
                    ("rotation", actual.rotations[i]),
                    ("position", actual.offsets[i]),
                ] {
                    for (got, want) in values
                        .into_iter()
                        .zip(row["parts"][name][field].as_array().unwrap())
                    {
                        assert!(
                            (f64::from(got) - want.as_f64().unwrap()).abs() < 1e-6,
                            "{name}/{field}: {row}"
                        );
                    }
                }
            }
        }
    }
}
