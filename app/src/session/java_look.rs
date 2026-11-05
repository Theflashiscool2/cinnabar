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

/// Position publication cannot carry a stale simulation angle back into input state.
pub(crate) fn publish_position(
    view: &mut client_presentation::local_player::LocalViewPose,
    position: [f64; 3],
    eye_height: f64,
) {
    let feet = bevy::prelude::Vec3::from_array(position.map(|v| v as f32));
    view.set_subject_position(feet + bevy::prelude::Vec3::Y * eye_height as f32, feet);
    view.reanchor_camera();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn position_publication_preserves_look_between_native_simulation_ticks() {
        let mut view = client_presentation::local_player::LocalViewPose::default();
        view.set_rotation(view_rotation([0., 18.]));
        // The player looks down after the last native tick, then the renderer receives
        // a position from that older tick. It must retain the new input until sampled.
        let wanted = view_rotation([0., 45.]);
        view.set_rotation(wanted);
        for step in 0..5 {
            publish_position(&mut view, [31.5, -60., -0.5 + step as f64 * 0.01], 1.62);
            assert_eq!(view.rotation(), wanted.normalize());
        }
        // A genuine correction still replaces angles through the explicit owner.
        view.set_rotation(view_rotation([90., 10.]));
        publish_position(&mut view, [32., -60., 2.], 1.62);
        let native = native_angles(view.rotation(), Some([90., 10.]));
        assert_eq!(native, [90., 10.]);
    }
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
