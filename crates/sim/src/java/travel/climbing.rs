//! Native LivingEntity.onClimbable and handleOnClimbable, using received block tags.
use super::*;
use assets::registry::java::physics::climbing::{self, Kind};

pub(super) fn context(position: [f64; 3], descending: bool) -> collision::PlayerContext {
    collision::PlayerContext {
        bottom: position[1],
        descending,
    }
}
pub(super) fn cell(terrain: &Terrain, p: [i32; 3]) -> Result<Option<u32>, Error> {
    let d = terrain.dimension().ok_or(QueryError::Retired)?;
    if p[1] < d.min_y || i64::from(p[1]) >= i64::from(d.min_y) + i64::from(d.height) {
        return Ok(None);
    }
    Ok(Some(terrain.block(p).ok_or(QueryError::Missing(p))?.0))
}
pub(super) fn is_scaffolding(
    position: [f64; 3],
    terrain: &Terrain,
    physics: &Table,
) -> Result<bool, Error> {
    let Some(id) = cell(terrain, position.map(|n| n.floor() as i32))? else {
        return Ok(false);
    };
    Ok(physics
        .registry()
        .is_some_and(|r| climbing::kind(r, id) == Some(Kind::Scaffolding)))
}
pub(super) fn at(
    position: [f64; 3],
    body: &mut Body,
    tags: &[i32],
    terrain: &Terrain,
    physics: &Table,
) -> Result<bool, Error> {
    let p = position.map(|n| n.floor() as i32);
    let Some(id) = cell(terrain, p)? else {
        return Ok(false);
    };
    let Some(r) = physics.registry() else {
        if tags.is_empty() {
            return Ok(false);
        }
        return Err(Error::Unsupported {
            behavior: "missing native climbing registry",
            position: p,
        });
    };
    let state = r.state(id).ok_or(QueryError::Unknown(id))?;
    let kind = climbing::kind(r, id).ok_or(Error::Unsupported {
        behavior: "missing native climbing class facts",
        position: p,
    })?;
    let tagged = r
        .block_id(id)
        .and_then(|id| i32::try_from(id).ok())
        .is_some_and(|id| tags.contains(&id));
    let climbable = if tagged {
        true
    } else if kind == Kind::Trapdoor && state.properties.get("open").is_some_and(|v| v == "true") {
        let below = [p[0], p[1] - 1, p[2]];
        if let Some(id) = cell(terrain, below)? {
            climbing::kind(r, id) == Some(Kind::Ladder)
                && r.state(id)
                    .is_some_and(|s| s.properties.get("facing") == state.properties.get("facing"))
        } else {
            false
        }
    } else {
        false
    };
    if climbable {
        body.last_climbable_position = Some(p);
    }
    Ok(climbable)
}
pub(super) fn clamp(velocity: &mut [f64; 3], shift: bool, scaffolding: bool) {
    let limit = f64::from(0.15_f32);
    for i in [0, 2] {
        velocity[i] = velocity[i].clamp(-limit, limit);
    }
    velocity[1] = velocity[1].max(-limit);
    if velocity[1] < 0. && shift && !scaffolding {
        velocity[1] = 0.;
    }
}
