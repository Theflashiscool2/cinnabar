//! Same-connection login replacement, matching the pinned ClientPacketListener.handleLogin.
use super::WorldState;
pub(super) fn admit(state: &mut WorldState, world: java_protocol::session::facts::World) -> bool {
    let Some(bootstrap) = state.bootstrap.as_mut() else {
        return false;
    };
    let previous = std::mem::replace(&mut bootstrap.world, world);
    if bootstrap.validate().is_err() {
        bootstrap.world = previous;
        state.terrain_error = Some("Java replacement world/dimension identity");
        return false;
    }
    let spawn = bootstrap.world.spawn.clone();
    // LocalPlayer.resetPos preserves X/Z/yaw, clears velocity and pitch, and resets the pose.
    // Keep this anchor private until the new server's position packet establishes authority.
    let anchor = state.player_motion.map(|mut motion| {
        motion.velocity = [0.; 3];
        motion.pitch = 0.;
        motion
    });
    if let Err(error) = super::terrain::login(state, &spawn) {
        state.terrain_error = Some(error);
        return false;
    }
    state.login_motion = anchor;
    state.players.clear_teams();
    state.player_visual.pose = "standing".into();
    state.death = None;
    state.vitals = None;
    state.respawn = None;
    true
}
#[cfg(test)]
mod tests;
