//! ScaffoldingBlock.getCollisionShape with the player's native collision context.
use super::*;
use assets::registry::java::physics::climbing;

/// No static-cube or empty fallback: only native scaffolding states use this resolver.
pub(super) fn shape(
    registry: &assets::registry::java::BlockRegistry,
    state: u32,
    position: [i32; 3],
    context: PlayerContext,
) -> Option<&'static [Bounds]> {
    let state = registry.state(state)?;
    let (stable, unstable, below) = climbing::scaffolding_shapes();
    let above =
        |height: f64| context.bottom > f64::from(position[1]) + height - f64::from(1.0e-5_f32);
    if above(1.) && !context.descending {
        return Some(stable);
    }
    let distance = state
        .properties
        .get("distance")?
        .parse::<u8>()
        .ok()
        .filter(|v| *v <= 7)?;
    let bottom = match state.properties.get("bottom")?.as_str() {
        "true" => true,
        "false" => false,
        _ => return None,
    };
    if distance != 0 && bottom && above(below) {
        Some(unstable)
    } else {
        Some(&[])
    }
}
