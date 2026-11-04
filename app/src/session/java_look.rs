//! Native angle/render bridge. Rendering must not manufacture new protocol rotations.
use bevy::prelude::{EulerRot, Quat};

pub(crate) fn view_rotation([yaw, pitch]: [f32; 2]) -> Quat {
    Quat::from_rotation_y((180. - yaw).to_radians()) * Quat::from_rotation_x(-pitch.to_radians())
}

pub(crate) fn native_angles(view: Quat, previous: Option<[f32; 2]>) -> [f32; 2] {
    if let Some(previous) = previous {
        let published = view_rotation(previous).normalize();
        if view == published || view == -published {
            return previous;
        }
    }
    let (yaw, pitch, _) = view.to_euler(EulerRot::YXZ);
    [180. - yaw.to_degrees(), -pitch.to_degrees()]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_angles_survive_repeated_render_publication_without_input() {
        for initial in [[0., 13.00000286102295_f32], [72.3, -27.8], [-3200.5, 89.]] {
            let mut native = initial;
            for _ in 0..300 {
                // LocalViewPose normalizes the published quaternion. A stationary native
                // player retains the exact float angles, including continuous yaw turns.
                native = native_angles(view_rotation(native).normalize(), Some(native));
                assert_eq!(native, initial);
            }
        }
    }
    #[test]
    fn actual_view_changes_are_not_filtered_by_an_epsilon() {
        let previous = [0., 13.];
        let changed = [0.001, 13.001];
        let actual = native_angles(view_rotation(changed).normalize(), Some(previous));
        assert_ne!(actual, previous);
        for i in 0..2 {
            assert!((actual[i] - changed[i]).abs() < 0.0001);
        }
    }
}
