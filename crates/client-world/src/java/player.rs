//! Applies Java corrections to the single local motion authority before acknowledging them.
use super::WorldState;
use java_protocol::session::{
    Command,
    facts::{Relative as WireRelative, Teleport},
};
use world::java::player::{Motion, Relative};

/// Last resolved server correction, distinct from the local motion subsequently predicted.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedTeleport {
    pub dimension_epoch: u64,
    pub id: i32,
    pub revision: u64,
    pub motion: Motion,
}

pub(super) fn admit(
    state: &mut WorldState,
    epoch: u64,
    revision: u64,
    correction: Teleport,
) -> Result<bool, &'static str> {
    if !state.terrain.accepts(state.generation(), epoch) || revision <= state.correction_sequence {
        return Ok(false);
    }
    let relative = Relative {
        position: [WireRelative::X, WireRelative::Y, WireRelative::Z]
            .map(|c| correction.is_relative(c)),
        velocity: [
            WireRelative::VelocityX,
            WireRelative::VelocityY,
            WireRelative::VelocityZ,
        ]
        .map(|c| correction.is_relative(c)),
        yaw: correction.is_relative(WireRelative::Yaw),
        pitch: correction.is_relative(WireRelative::Pitch),
        rotate_velocity: correction.is_relative(WireRelative::RotateVelocity),
    };
    if state.player_motion.is_none()
        && (relative.position.into_iter().any(|v| v) || relative.yaw || relative.pitch)
    {
        return Err("initial relative Java teleport has no admitted player anchor");
    }
    let anchor = state.player_motion.unwrap_or_else(|| Motion {
        velocity: state.player_dynamics.unplaced_velocity.unwrap_or_default(),
        ..Default::default()
    });
    let motion = anchor.corrected(
        Motion {
            position: correction.position,
            velocity: correction.velocity,
            yaw: correction.yaw,
            pitch: correction.pitch,
        },
        relative,
    )?;
    state.player_pose_revision = state
        .player_pose_revision
        .checked_add(1)
        .ok_or("Java player pose revision overflow")?;
    state.player_dynamics.unplaced_velocity = None;
    state.correction_sequence = revision;
    state.player_motion = Some(motion);
    state.teleport = Some(ResolvedTeleport {
        dimension_epoch: epoch,
        id: correction.id,
        revision,
        motion,
    });
    state.pending_correction = Some(revision);
    Ok(true)
}

impl WorldState {
    /// Prediction commands are valid only while this server correction receipt remains current.
    pub fn motion_sequence(&self) -> u64 {
        self.correction_sequence
    }
    /// A full transport queue retains this command for the next frame; callers mark it only
    /// after enqueue succeeds. A retired receipt cannot clear a newer correction.
    pub fn correction_command(&self) -> Option<(u64, Command)> {
        if let Some((dimension_epoch, sequence, yaw, pitch)) = self.pending_rotation {
            return Some((
                sequence,
                Command::AcceptRotation {
                    dimension_epoch,
                    sequence,
                    yaw,
                    pitch,
                },
            ));
        }
        let correction = self.teleport.as_ref()?;
        if self.pending_correction != Some(correction.revision) {
            return None;
        }
        Some((
            correction.revision,
            Command::AcceptTeleport {
                dimension_epoch: correction.dimension_epoch,
                sequence: correction.revision,
                id: correction.id,
                position: correction.motion.position,
                yaw: correction.motion.yaw,
                pitch: correction.motion.pitch,
            },
        ))
    }
    pub fn correction_queued(&mut self, revision: u64) {
        if self
            .pending_rotation
            .is_some_and(|(_, sequence, _, _)| sequence == revision)
        {
            self.pending_rotation = None;
        }
        if self.pending_correction == Some(revision) {
            self.pending_correction = None;
        }
    }
    pub fn awaiting_correction_ack(&self) -> bool {
        self.pending_correction.is_some() || self.pending_rotation.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use java_protocol::session::{Envelope, Event};
    fn state() -> WorldState {
        let mut state = WorldState::default();
        state.begin(7);
        state.terrain.begin(world::java::Dimension {
            session: 7,
            epoch: 2,
            name: "fixture:dimension".into(),
            min_y: -64,
            height: 384,
        });
        state
    }
    fn correction(generation: u64, epoch: u64, sequence: u64, relative: u32) -> Envelope {
        Envelope {
            generation,
            event: Event::Teleport {
                dimension_epoch: epoch,
                sequence,
                correction: Teleport {
                    id: 5,
                    position: [1., 64., 2.],
                    velocity: [0.; 3],
                    yaw: 0.,
                    pitch: 0.,
                    relative,
                },
            },
        }
    }
    #[test]
    fn corrections_use_current_prediction_and_old_receipts_cannot_clear_new_authority() {
        let mut state = state();
        assert!(state.admit(correction(7, 2, 1, 0)));
        let (first, _) = state.correction_command().unwrap();
        // Prediction has moved beyond the last server correction; relative correction uses it.
        state.player_motion.as_mut().unwrap().position = [10., 65., 20.];
        assert!(state.admit(correction(7, 2, 2, 7)));
        assert_eq!(state.player_motion.unwrap().position, [11., 129., 22.]);
        state.correction_queued(first);
        let (second, Command::AcceptTeleport { position, .. }) =
            state.correction_command().unwrap()
        else {
            panic!("expected acknowledgement")
        };
        assert_eq!(position, [11., 129., 22.]);
        assert_ne!(first, second); // A reused server id still replaces camera/collision authority.
        state.correction_queued(second);
        assert!(!state.awaiting_correction_ack());
    }
    #[test]
    fn old_session_and_dimension_corrections_do_not_mutate_the_new_world() {
        let mut state = state();
        assert!(!state.admit(correction(6, 2, 2, 0)));
        assert!(!state.admit(correction(7, 1, 2, 0)));
        assert!(state.player_motion.is_none());
        assert!(!state.awaiting_correction_ack());
        assert!(state.admit(correction(7, 2, 1, 0)));
        state.begin(8);
        assert!(state.player_motion.is_none());
        assert!(state.correction_command().is_none());
    }
    #[test]
    fn missing_anchor_and_malformed_correction_do_not_admit_authority() {
        let mut state = state();
        assert!(!state.admit(correction(7, 2, 1, 7)));
        assert!(state.player_motion.is_none());
        assert!(state.correction_command().is_none());
        state.terrain_error = None;
        assert!(state.admit(correction(7, 2, 2, 0)));
        let before = state.player_motion;
        assert!(!state.admit(correction(7, 2, 1, 0)));
        let mut invalid = correction(7, 2, 3, 0);
        if let Event::Teleport { correction, .. } = &mut invalid.event {
            correction.position[0] = f64::NAN;
        }
        assert!(!state.admit(invalid));
        assert_eq!(state.player_motion, before);
        assert_eq!(state.correction_command().unwrap().0, 2);
    }
}
