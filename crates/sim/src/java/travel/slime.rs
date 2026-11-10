//! Pinned SlimeBlock.stepOn and entity/block collision restitution; damage stays server-owned.
use super::*;

pub(super) fn is_slime(physics: &Table, id: u32) -> bool {
    physics
        .registry()
        .and_then(|r| assets::registry::java::physics::callbacks::kind(r, id))
        == Some(assets::registry::java::physics::callbacks::Kind::Slime)
}
pub(super) fn contact_cell(position: [f64; 3], support: Option<[i32; 3]>) -> [i32; 3] {
    [
        support.map_or(position[0].floor() as i32, |p| p[0]),
        (position[1] - f64::from(0.2_f32)).floor() as i32,
        support.map_or(position[2].floor() as i32, |p| p[2]),
    ]
}
pub(super) fn suppressed(
    terrain: &Terrain,
    physics: &Table,
    position: [f64; 3],
    support: Option<[i32; 3]>,
    tags: &[i32],
) -> Result<bool, Error> {
    if tags.is_empty() {
        return Ok(false);
    }
    let cell = contact_cell(position, support);
    let Some(id) = climbing::cell(terrain, cell)? else {
        return Ok(false);
    };
    let block = physics
        .registry()
        .and_then(|r| r.block_id(id))
        .and_then(|n| i32::try_from(n).ok())
        .ok_or(Error::Unsupported {
            behavior: "missing native bounce tag registry",
            position: cell,
        })?;
    Ok(tags.contains(&block))
}
/// Fractional contact compensates gravity/drag for the untraveled portion of the tick.
/// Called only on a vertical collision; the caller selects entity/block restitution.
pub(super) fn restitute_y(
    incoming: f64,
    actual: f64,
    below: bool,
    suppress: bool,
    restitution: f64,
    gravity: f64,
    air_drag: f32,
) -> f64 {
    let bounce = if (!below || -incoming > gravity) && !suppress {
        restitution
    } else {
        0.
    };
    let (g, drag) = if bounce > 0. {
        let fraction = actual / incoming;
        (
            fraction * gravity,
            1. + fraction * (f64::from(air_drag) - 1.),
        )
    } else {
        (0., 1.)
    };
    (g - incoming) * drag * bounce
}
/// This callback runs after travel's gravity/drag, only while grounded.
pub(super) fn step_on(velocity: &mut [f64; 3], careful: bool) {
    let speed = velocity[1].abs();
    if speed < 0.1 && !careful {
        let factor = 0.4 + speed * 0.2;
        velocity[0] *= factor;
        velocity[2] *= factor;
    }
}
