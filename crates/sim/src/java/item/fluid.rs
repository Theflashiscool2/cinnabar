//! ItemEntity.setFluidMovement from the pinned 26.3 runtime, separate from living travel drag.

/// The base tick has already applied currents. Items float independently of no-gravity metadata;
/// shallow contacts fall through to ordinary gravity, with water taking precedence over lava.
pub(super) fn movement(velocity: &mut [f64; 3], no_gravity: bool, water: f64, lava: f64) {
    let damping = if water > f64::from(0.1_f32) {
        Some(0.99_f32)
    } else if lava > f64::from(0.1_f32) {
        Some(0.95_f32)
    } else {
        None
    };
    if let Some(damping) = damping {
        velocity[0] *= f64::from(damping);
        velocity[2] *= f64::from(damping);
        if velocity[1] < f64::from(0.06_f32) {
            velocity[1] += f64::from(0.0005_f32);
        }
    } else if !no_gravity {
        velocity[1] -= 0.04;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_depth_threshold_water_precedence_and_no_gravity_buoyancy() {
        let mut shallow = [0.1, 0., -0.2];
        movement(&mut shallow, false, f64::from(0.1_f32), 0.);
        assert_eq!(shallow, [0.1, -0.04, -0.2]);
        let mut water = [0.1, 0., -0.2];
        movement(&mut water, true, 0.2, 0.2);
        assert_eq!(
            water,
            [
                0.1 * f64::from(0.99_f32),
                f64::from(0.0005_f32),
                -0.2 * f64::from(0.99_f32)
            ]
        );
        let mut lava = [0.1, 0., -0.2];
        movement(&mut lava, false, f64::from(0.1_f32), 0.2);
        assert_eq!(
            lava,
            [
                0.1 * f64::from(0.95_f32),
                f64::from(0.0005_f32),
                -0.2 * f64::from(0.95_f32)
            ]
        );
    }
    #[test]
    fn native_buoyancy_ceiling_tests_incoming_vertical_speed_without_clamping() {
        let ceiling = f64::from(0.06_f32);
        let mut at = [0., ceiling, 0.];
        movement(&mut at, false, 0.2, 0.);
        assert_eq!(at[1], ceiling);
        let mut below = [0., ceiling - 0.0001, 0.];
        movement(&mut below, false, 0.2, 0.);
        assert_eq!(below[1], ceiling - 0.0001 + f64::from(0.0005_f32));
        assert!(
            below[1] > ceiling,
            "native addition can overshoot the threshold"
        );
    }
}
