//! Native Java player motion authority. Packet masks are interpreted at admission, not here.
//!
//! Correction semantics follow the pinned client's PositionMoveRotation.calculateAbsolute,
//! Vec3 rotations and Mth trigonometric lookup. Coordinates and velocity retain f64 precision.
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Relative {
    pub position: [bool; 3],
    pub velocity: [bool; 3],
    pub yaw: bool,
    pub pitch: bool,
    pub rotate_velocity: bool,
}

impl Motion {
    pub fn is_finite(self) -> bool {
        self.position
            .iter()
            .chain(&self.velocity)
            .all(|n| n.is_finite())
            && self.yaw.is_finite()
            && self.pitch.is_finite()
    }

    /// Replaces authority atomically, resolving relative fields against the current local motion.
    /// Rotated velocity is calculated before component-wise relative velocity is applied.
    pub fn corrected(self, change: Self, relative: Relative) -> Result<Self, &'static str> {
        if !self.is_finite() || !change.is_finite() {
            return Err("nonfinite Java player correction");
        }
        let mut next = Self {
            position: std::array::from_fn(|i| {
                change.position[i]
                    + if relative.position[i] {
                        self.position[i]
                    } else {
                        0.
                    }
            }),
            yaw: change.yaw + if relative.yaw { self.yaw } else { 0. },
            pitch: change.pitch + if relative.pitch { self.pitch } else { 0. },
            velocity: change.velocity,
        };
        // Check rotation overflow before clamping: clamping must not hide malformed authority.
        if !next.is_finite() {
            return Err("Java player correction overflow");
        }
        next.pitch = next.pitch.clamp(-90., 90.);
        let mut velocity = self.velocity;
        if relative.rotate_velocity {
            let pitch = f64::from(self.pitch - next.pitch).to_radians() as f32;
            let yaw = f64::from(self.yaw - next.yaw).to_radians() as f32;
            if !pitch.is_finite() || !yaw.is_finite() {
                return Err("Java player correction rotation overflow");
            }
            let [sin, cos] = trig(pitch);
            let [x, y, z] = velocity;
            velocity = [x, y * cos + z * sin, z * cos - y * sin];
            let [sin, cos] = trig(yaw);
            let [x, y, z] = velocity;
            velocity = [x * cos + z * sin, y, z * cos - x * sin];
        }
        for (i, value) in next.velocity.iter_mut().enumerate() {
            if relative.velocity[i] {
                *value += velocity[i];
            }
        }
        if next.is_finite() {
            Ok(next)
        } else {
            Err("Java player correction overflow")
        }
    }
}

/// Java's table uses double-precision indexing and table initialization, unlike Bedrock's
/// simulation helper. Only the table values and rotation argument are rounded to float.
fn trig(radians: f32) -> [f64; 2] {
    const SIZE: usize = 65536;
    const SCALE: f64 = SIZE as f64 / std::f64::consts::TAU;
    static TABLE: OnceLock<Box<[f32; SIZE]>> = OnceLock::new();
    let table =
        TABLE.get_or_init(|| Box::new(std::array::from_fn(|i| (i as f64 / SCALE).sin() as f32)));
    let index = f64::from(radians) * SCALE;
    [index, index + (SIZE / 4) as f64]
        .map(|n| f64::from(table[((n as i64) & (SIZE as i64 - 1)) as usize]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_and_relative_components_preserve_java_coordinate_precision() {
        let before = Motion {
            position: [1_073_741_824., -61., 8.],
            velocity: [1., 2., 3.],
            yaw: 20.,
            pitch: 60.,
        };
        let next = before
            .corrected(
                Motion {
                    position: [0.125, 70., -2.],
                    velocity: [0.5, 0., -1.],
                    yaw: -30.,
                    pitch: 50.,
                },
                Relative {
                    position: [true, false, true],
                    velocity: [true, false, true],
                    yaw: true,
                    pitch: true,
                    rotate_velocity: false,
                },
            )
            .unwrap();
        assert_eq!(next.position, [1_073_741_824.125, 70., 6.]);
        assert_eq!(next.velocity, [1.5, 0., 2.]);
        assert_eq!([next.yaw, next.pitch], [-10., 90.]);
    }

    #[test]
    fn rotated_velocity_precedes_relative_velocity_and_absolute_components_replace_it() {
        let before = Motion {
            velocity: [1., 2., 3.],
            ..Default::default()
        };
        let next = before
            .corrected(
                Motion {
                    velocity: [0.5, 7., 0.],
                    yaw: 90.,
                    ..Default::default()
                },
                Relative {
                    velocity: [true, false, true],
                    rotate_velocity: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(next.velocity, [-2.5, 7., 1.]);
    }

    #[test]
    fn rotated_velocity_matches_pinned_vanilla_independent_vector() {
        // Evaluated with PositionMoveRotation.calculateAbsolute in the hash-pinned vanilla
        // server archive. Mixed absolute/relative components exercise both rotation axes.
        let before = Motion {
            position: [10_000_000.125, -61.5, 9.25],
            velocity: [0.12345, -0.875, 3.21],
            yaw: 137.25,
            pitch: -65.5,
        };
        let next = before
            .corrected(
                Motion {
                    position: [0.25, 66., -4.5],
                    velocity: [0.125, -0.25, 4.],
                    yaw: -75.125,
                    pitch: 87.25,
                },
                Relative {
                    position: [true, false, true],
                    velocity: [true, true, false],
                    pitch: true,
                    rotate_velocity: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(next.position, [10_000_000.375, 66., 4.75]);
        assert_eq!(
            next.velocity.map(f64::to_bits),
            [
                0x3fda_0117_3d75_19db,
                0xc00b_fc5e_2921_47ae,
                0x4010_0000_0000_0000,
            ]
        );
        assert_eq!([next.yaw, next.pitch], [-75.125, 21.75]);
    }

    #[test]
    fn malformed_or_overflowing_authority_never_changes_motion() {
        let before = Motion {
            position: [f64::MAX, 0., 0.],
            ..Default::default()
        };
        assert!(
            before
                .corrected(
                    before,
                    Relative {
                        position: [true; 3],
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert!(
            before
                .corrected(
                    Motion {
                        yaw: f32::NAN,
                        ..Default::default()
                    },
                    Relative::default()
                )
                .is_err()
        );
        assert_eq!(before.position[0], f64::MAX);
    }
}
