//! Pinned Player.travel/updatePlayerPose and Avatar dimensions. The swimming flag is
//! sampled before input/travel; collision pose selection runs after travel.
use super::*;

pub(super) fn fits(
    position: [f64; 3],
    low: bool,
    crouched: bool,
    scale: f32,
    terrain: &Terrain,
    physics: &Table,
    scope: (u64, u64),
) -> Result<bool, Error> {
    let mut shape = Body {
        low_profile: low,
        crouched,
        ..Default::default()
    }
    .bounds(position, scale);
    for i in 0..3 {
        shape.min[i] += 1.0e-7;
        shape.max[i] -= 1.0e-7;
    }
    let query = collision::collect(terrain, physics, scope.0, scope.1, shape)?;
    Ok(!query.boxes.iter().any(|b| collision::intersects(shape, *b)))
}

pub(super) fn finish_pose(
    body: &mut Body,
    position: [f64; 3],
    shift: bool,
    scale: f32,
    terrain: &Terrain,
    physics: &Table,
    scope: (u64, u64),
) -> Result<(), Error> {
    if !fits(position, true, false, scale, terrain, physics, scope)? {
        return Ok(());
    }
    let desired = (body.swimming, !body.swimming && shift);
    let pose = if fits(
        position, desired.0, desired.1, scale, terrain, physics, scope,
    )? {
        desired
    } else if fits(position, false, true, scale, terrain, physics, scope)? {
        (false, true)
    } else {
        (true, false)
    };
    body.low_profile = pose.0;
    body.crouched = pose.1;
    Ok(())
}

pub(super) fn steer(
    motion: &mut Motion,
    jump: bool,
    cells: &liquid::Cells<'_>,
) -> Result<(), Error> {
    let [sin, _] = trig(motion.pitch * (std::f32::consts::PI / 180.));
    let look_y = -sin;
    if look_y <= 0.
        || jump
        || cells.has_fluid_at([
            motion.position[0],
            motion.position[1] + 1. - 0.1,
            motion.position[2],
        ])?
    {
        motion.velocity[1] +=
            (look_y - motion.velocity[1]) * if look_y < -0.2 { 0.085 } else { 0.06 };
    }
    Ok(())
}
