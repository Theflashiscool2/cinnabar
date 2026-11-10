//! Ordered entity-inside sweeps for bubble columns, honey and cobweb contacts.
use super::*;
use std::collections::HashSet;

pub(super) fn is_column(physics: &Table, id: u32) -> bool {
    physics
        .registry()
        .and_then(|r| assets::registry::java::physics::callbacks::kind(r, id))
        == Some(assets::registry::java::physics::callbacks::Kind::BubbleColumn)
}
pub(super) fn server_damage(physics: &Table, id: u32) -> bool {
    physics
        .registry()
        .and_then(|r| assets::registry::java::physics::callbacks::kind(r, id))
        == Some(assets::registry::java::physics::callbacks::Kind::ServerDamage)
}
/// Apply the post-travel axis-ordered block callbacks once per visited block.
/// Missing terrain or an unresolved ceiling shape aborts the entire simulation tick.
pub(super) fn apply(
    from: [f64; 3],
    original: [f64; 3],
    motion: &mut Motion,
    body: &mut Body,
    pose: Body,
    scale: f32,
    weaving: bool,
    terrain: &Terrain,
    physics: &Table,
) -> Result<(), Error> {
    let mut seen = HashSet::new();
    let delta = std::array::from_fn::<_, 3, _>(|i| motion.position[i] - from[i]);
    let mut cursor = from;
    let mut budget = 16;
    if delta.iter().any(|n| *n != 0.) {
        for axis in collision::sweep::axes(original) {
            if delta[axis] == 0. {
                continue;
            }
            let mut to = cursor;
            to[axis] += delta[axis];
            budget -= segment(
                cursor,
                to,
                pose.bounds(to, scale),
                budget,
                &mut seen,
                motion,
                body,
                scale,
                weaving,
                terrain,
                physics,
            )?;
            cursor = to;
        }
    } else {
        budget -= segment(
            from,
            from,
            pose.bounds(from, scale),
            budget,
            &mut seen,
            motion,
            body,
            scale,
            weaving,
            terrain,
            physics,
        )?;
    }
    if budget <= 0 {
        segment(
            motion.position,
            motion.position,
            pose.bounds(motion.position, scale),
            1,
            &mut seen,
            motion,
            body,
            scale,
            weaving,
            terrain,
            physics,
        )?;
    }
    Ok(())
}
fn segment(
    from: [f64; 3],
    to: [f64; 3],
    mut bounds: Bounds,
    budget: i32,
    seen: &mut HashSet<[i32; 3]>,
    motion: &mut Motion,
    body: &mut Body,
    scale: f32,
    weaving: bool,
    terrain: &Terrain,
    physics: &Table,
) -> Result<i32, Error> {
    let eps = f64::from(1e-5_f32);
    for i in 0..3 {
        bounds.min[i] += eps;
        bounds.max[i] -= eps;
    }
    let far = 0.999_990_000_000_252_6_f64;
    let precise = (0..3).map(|i| (to[i] - from[i]).powi(2)).sum::<f64>() > far * far;
    let mut steps = 0;
    for (pos, step) in collision::sweep::visits(from, to, bounds)? {
        if step >= budget {
            break;
        }
        steps = step;
        let Some(id) = cell(terrain, pos)? else {
            continue;
        };
        match sticky::kind(physics, id) {
            Some(assets::registry::java::physics::callbacks::Kind::Honey) => {
                if seen.insert(pos) {
                    sticky::honey(motion, body, pos, scale);
                }
                continue;
            }
            Some(assets::registry::java::physics::callbacks::Kind::Cobweb) => {
                if seen.insert(pos) {
                    sticky::web(body, weaving);
                }
                continue;
            }
            _ => {}
        }
        // BubbleColumnBlock alone requires precise entity-inside contact.
        if !is_column(physics, id)
            || (!precise
                && !(0..3).all(|i| {
                    bounds.max[i] > f64::from(pos[i]) && bounds.min[i] < f64::from(pos[i]) + 1.
                }))
        {
            continue;
        }
        if !seen.insert(pos) {
            continue;
        }
        let state = physics
            .registry()
            .and_then(|r| r.state(id))
            .ok_or(QueryError::Unknown(id))?;
        let drag = match state.properties.get("drag").map(String::as_str) {
            Some("true") => true,
            Some("false") => false,
            _ => {
                return Err(Error::Unsupported {
                    behavior: "invalid bubble-column drag state",
                    position: pos,
                });
            }
        };
        let above = [pos[0], pos[1] + 1, pos[2]];
        let surface = if let Some(id) = cell(terrain, above)? {
            let state = physics.state(id).ok_or(QueryError::Unknown(id))?;
            if !state.fluid_empty {
                false
            } else {
                match &state.collision {
                    assets::registry::java::physics::Collision::Static(boxes) => boxes.is_empty(),
                    assets::registry::java::physics::Collision::Dynamic
                        if physics.registry().is_some_and(|r| {
                            assets::registry::java::physics::climbing::kind(r, id)
                                == Some(
                                    assets::registry::java::physics::climbing::Kind::Scaffolding,
                                )
                        }) =>
                    {
                        // Empty CollisionContext selects the stable native scaffold shape,
                        // independently of the player's below-platform/descending context.
                        assets::registry::java::physics::climbing::scaffolding_shapes()
                            .0
                            .is_empty()
                    }
                    _ => {
                        return Err(Error::Unsupported {
                            behavior: "bubble-column contextual ceiling shape",
                            position: above,
                        });
                    }
                }
            }
        } else {
            true
        };
        impulse(
            &mut motion.velocity[1],
            &mut body.fall_distance,
            drag,
            surface,
        );
    }
    Ok(steps + 1)
}
fn cell(terrain: &Terrain, p: [i32; 3]) -> Result<Option<u32>, Error> {
    let d = terrain.dimension().ok_or(QueryError::Retired)?;
    if p[1] < d.min_y || i64::from(p[1]) >= i64::from(d.min_y) + i64::from(d.height) {
        return Ok(None);
    }
    Ok(Some(terrain.block(p).ok_or(QueryError::Missing(p))?.0))
}
pub(super) fn impulse(y: &mut f64, fall: &mut f64, drag: bool, surface: bool) {
    *y = match (surface, drag) {
        (true, true) => (*y - 0.03).max(-0.9),
        (true, false) => (*y + 0.1).min(1.8),
        (false, true) => (*y - 0.03).max(-0.3),
        (false, false) => (*y + 0.06).min(0.7),
    };
    if !surface {
        *fall = 0.;
    }
}
