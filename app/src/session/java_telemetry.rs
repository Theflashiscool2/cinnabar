//! Passive hooks shared by main-frame and lent-worker admission. No gameplay decisions.
use super::SessionController;
use bevy::prelude::*;
use client_session::java::{
    Session,
    telemetry::{Counter, Recorder, Scope, Stage},
};
use java_protocol::session::{Envelope, Event, entities::Packet};
use std::sync::Arc;
use web_time::Instant;

pub(super) fn admit(
    java: &Session,
    state: &mut client_world::java::WorldState,
    event: Envelope,
) -> bool {
    let Some(t) = java.telemetry() else {
        return state.admit(event);
    };
    let teleport = matches!(event.event, Event::Teleport { .. });
    let inventory = matches!(event.event, Event::Inventory { .. });
    let ack = matches!(event.event, Event::BlockAck { .. });
    let receipt = match &event.event {
        Event::InventorySent {
            written, closed, ..
        } if !closed => Some(*written),
        _ => None,
    };
    let had_swing = state.player_visual.swing.sample(1.).is_some();
    let before = state.player_motion.map(|m| m.position);
    let blocks = state.block_updates;
    let unknown = state.entities.unknown_updates;
    let current = state.terrain.dimension().map(|d| (d.session, d.epoch));
    let local_collision = matches!(&event.event, Event::Entities{dimension_epoch,packet:Packet::Spawn(spawn)} if current==Some((event.generation,*dimension_epoch)) && state.bootstrap.as_ref().is_some_and(|b|b.world.entity_id==spawn.id));
    let (spawn_collision, unknown_removals) = match &event.event {
        Event::Entities {
            dimension_epoch,
            packet,
        } if current == Some((event.generation, *dimension_epoch)) => match packet {
            Packet::Spawn(s) => (u64::from(state.entities.get(s.id).is_some()), 0),
            Packet::Remove(ids) => (
                0,
                ids.iter()
                    .filter(|id| state.entities.get(**id).is_none())
                    .count() as u64,
            ),
            _ => (0, 0),
        },
        _ => (0, 0),
    };
    let admitted = state.admit(event);
    if admitted && had_swing && state.player_visual.swing.sample(1.).is_none() {
        t.count(Counter::SwingWorldClear, 1);
    }
    if !admitted {
        t.count(Counter::StaleWorldEvents, 1);
    }
    if teleport {
        if admitted {
            let distance = before.zip(state.player_motion).map(|(a, b)| {
                (b.position[0] - a[0])
                    .hypot(b.position[1] - a[1])
                    .hypot(b.position[2] - a[2])
            });
            t.correction(distance, before.is_none(), state.inventory.opened.is_some());
        } else {
            t.count(Counter::TeleportsRefused, 1);
        }
    }
    if inventory && !admitted {
        t.count(Counter::ContainerAuthorityRefused, 1);
    }
    if ack && admitted {
        t.count(Counter::BlockAcks, 1);
    }
    if let Some(written) = receipt {
        t.count(
            if written {
                Counter::ContainerWritten
            } else {
                Counter::ContainerNotWritten
            },
            1,
        );
    }
    t.count(
        Counter::BlockUpdates,
        state.block_updates.saturating_sub(blocks),
    );
    t.count(
        Counter::EntityUnknownUpdates,
        state.entities.unknown_updates.saturating_sub(unknown),
    );
    t.count(Counter::EntityLocalCollision, u64::from(local_collision));
    t.count(Counter::EntitySpawnCollision, spawn_collision);
    t.count(Counter::EntityUnknownRemovals, unknown_removals);
    admitted
}

pub(super) fn scope(controller: &SessionController, stage: Stage) -> Option<Scope> {
    controller.java_telemetry().map(|t| Scope::new(t, stage))
}

/// Tracks active animation publication separately from intentional camera/menu visibility.
pub(super) struct HandTrace {
    recorder: Arc<Recorder>,
    active: bool,
    pub published: bool,
    pub hidden: bool,
    pub cause: client_session::java::telemetry::HandIssue,
}
impl HandTrace {
    pub fn new(controller: &SessionController, using: bool) -> Option<Self> {
        let recorder = Arc::clone(controller.java_telemetry.as_ref()?);
        let active = using
            || controller
                .java_world
                .player_visual
                .swing
                .sample(1.)
                .is_some_and(|s| s.kind != world::java::entities::animation::swing::Kind::None);
        Some(Self {
            recorder,
            active,
            published: false,
            hidden: false,
            cause: client_session::java::telemetry::HandIssue::Owners,
        })
    }
}
impl Drop for HandTrace {
    fn drop(&mut self) {
        self.recorder
            .hand_frame(self.active, self.published, self.hidden, self.cause);
    }
}

#[derive(Resource, Default)]
struct Frame {
    recorder: Option<Arc<Recorder>>,
    previous: Option<Instant>,
    start: Option<Instant>,
}
pub(crate) fn configure(app: &mut App) {
    app.init_resource::<Frame>()
        .add_systems(render::FrameStart, begin)
        .add_systems(Last, end);
}
fn begin(mut frame: ResMut<Frame>, controller: Option<Res<SessionController>>) {
    let next = controller
        .as_ref()
        .and_then(|c| c.java_telemetry.as_ref())
        .cloned();
    let same = next
        .as_ref()
        .zip(frame.recorder.as_ref())
        .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
    if !same {
        frame.previous = None;
    }
    frame.recorder = next;
    if frame.recorder.is_none() {
        frame.previous = None;
        frame.start = None;
        return;
    }
    let now = Instant::now();
    if let Some(t) = frame.recorder.as_ref() {
        if let Some(previous) = frame.previous {
            t.measure(
                Stage::FrameInterval,
                now.saturating_duration_since(previous),
            );
        }
    }
    frame.previous = Some(now);
    frame.start = Some(now);
}
fn end(
    mut frame: ResMut<Frame>,
    controller: Option<Res<SessionController>>,
    interaction: Option<Res<super::java_interaction::Interaction>>,
    terrain: Option<Res<super::java_render::Presentation>>,
    entities: Option<Res<super::java_entities::Presentation>>,
) {
    let next = controller
        .as_ref()
        .and_then(|c| c.java_telemetry.as_ref())
        .cloned();
    let same = next
        .as_ref()
        .zip(frame.recorder.as_ref())
        .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
    if !same {
        frame.previous = None;
        frame.start = None;
    }
    frame.recorder = next;
    if let Some(t) = frame.recorder.as_ref() {
        let [chunks, textures] = terrain
            .as_ref()
            .map_or([false; 2], |p| p.telemetry_activity());
        t.loading([
            chunks,
            entities.as_ref().is_some_and(|p| p.telemetry_loading()),
            textures,
        ]);
        if let Some(start) = frame.start {
            t.measure(Stage::MainUpdate, start.elapsed());
        }
        if let Some(c) = controller.as_ref().filter(|c| c.java.is_some())
            && let Some(d) = c.java_world.terrain.dimension()
        {
            t.action_state(
                (d.session, d.epoch),
                interaction.as_ref().is_some_and(|i| i.0.using_item()),
                interaction
                    .as_ref()
                    .is_some_and(|i| i.0.mining_progress().is_some()),
            );
        }
    }
}

#[cfg(test)]
mod tests;
