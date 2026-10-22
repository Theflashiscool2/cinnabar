use super::*;
use java_protocol::session::{Command, Envelope, Event, facts};

#[test]
fn native_player_builder_defaults_preserve_exact_values_and_do_not_include_mob_attributes() {
    let defaults = initial();
    assert_eq!(
        defaults.attributes["minecraft:movement_speed"].base,
        f64::from(0.1_f32)
    );
    assert_eq!(
        defaults.attributes["minecraft:jump_strength"].base,
        f64::from(0.42_f32)
    );
    assert_eq!(defaults.attributes["minecraft:gravity"].base, 0.08);
    assert!(!defaults.attributes.contains_key("minecraft:follow_range"));
    assert!(defaults.abilities.is_none());
}

use crate::java::tests_support::state;

fn event(generation: u64, dimension_epoch: u64, packet: wire::Packet) -> Envelope {
    Envelope {
        generation,
        event: Event::Dynamics {
            dimension_epoch,
            packet,
        },
    }
}
fn velocity(entity: i32) -> wire::Packet {
    wire::Packet::Velocity {
        entity,
        velocity: [1., 2., 3.],
    }
}

#[test]
fn velocity_requires_current_scope_and_identity_without_inventing_a_spawn() {
    let mut s = state();
    assert!(!s.admit(event(6, 1, velocity(42))));
    assert!(!s.admit(event(7, 2, velocity(42))));
    assert!(!s.admit(event(7, 1, velocity(99))));
    assert!(s.player_motion.is_none());
    assert!(s.player_dynamics.unplaced_velocity.is_none());
    assert_eq!(s.entities.unknown_updates, 1);
    assert!(s.entities.get(99).is_none());
    assert!(s.admit(event(7, 1, velocity(42))));
    assert!(s.player_motion.is_none());
    assert_eq!(s.player_dynamics.unplaced_velocity, Some([1., 2., 3.]));
    assert!(s.admit(Envelope {
        generation: 7,
        event: Event::Teleport {
            dimension_epoch: 1,
            sequence: 1,
            correction: facts::Teleport {
                id: 8,
                position: [2., 64., 3.],
                velocity: [0.; 3],
                yaw: 0.,
                pitch: 0.,
                relative: facts::Relative::VelocityX.mask(),
            },
        }
    }));
    assert_eq!(s.player_motion.unwrap().velocity, [1., 0., 0.]);
    assert!(s.player_dynamics.unplaced_velocity.is_none());
    s.begin(8);
    assert!(s.player_motion.is_none());
    assert!(s.player_dynamics.attributes.is_empty());
}

#[test]
fn rotation_uses_current_motion_and_receipts_cannot_acknowledge_another_correction() {
    let mut s = state();
    s.player_motion = Some(Motion {
        position: [1., 64., 2.],
        velocity: [0.1, 0.2, 0.3],
        yaw: 30.,
        pitch: 80.,
    });
    let rotation = |sequence| Envelope {
        generation: 7,
        event: Event::Rotation {
            dimension_epoch: 1,
            sequence,
            yaw: 90.,
            relative_yaw: true,
            pitch: 30.,
            relative_pitch: true,
        },
    };
    assert!(s.admit(rotation(1)));
    assert_eq!(s.player_motion.unwrap().yaw, 120.);
    assert_eq!(s.player_motion.unwrap().pitch, 90.);
    assert_eq!(s.player_motion.unwrap().velocity, [0.1, 0.2, 0.3]);
    assert!(s.teleport.is_none());
    assert!(s.admit(rotation(2)));
    s.correction_queued(1);
    let (
        revision,
        Command::AcceptRotation {
            yaw,
            pitch,
            sequence,
            ..
        },
    ) = s.correction_command().unwrap()
    else {
        panic!()
    };
    assert_eq!((yaw, pitch, sequence), (210., 90., 2));
    assert!(!s.admit(rotation(1)));
    s.correction_queued(revision);
    assert!(!s.awaiting_correction_ack());
    assert_eq!(s.player_pose_revision, 2);
}

#[test]
fn attribute_updates_are_atomic_and_preserve_unmentioned_attributes() {
    let mut s = state();
    let attribute = |name, base| wire::Attribute {
        name,
        base,
        minimum: 0.,
        maximum: 10.,
        modifiers: vec![],
    };
    let update = |values| event(7, 1, wire::Packet::Attributes { entity: 42, values });
    assert!(s.admit(update(vec![attribute("minecraft:movement_speed", 0.1)])));
    assert!(s.admit(update(vec![attribute("minecraft:gravity", 0.08)])));
    assert_eq!(s.player_dynamics.attributes.len(), 2);
    assert!(!s.admit(update(vec![
        attribute("minecraft:movement_speed", 1.),
        attribute("minecraft:gravity", f64::NAN)
    ])));
    assert_eq!(
        s.player_dynamics.attributes["minecraft:movement_speed"].value(),
        Ok(0.1)
    );
}

#[test]
fn respawn_preserves_bases_and_obeys_modifier_retention() {
    let mut s = state();
    let attribute = wire::Attribute {
        name: "minecraft:movement_speed",
        base: 0.1,
        minimum: 0.,
        maximum: 1024.,
        modifiers: vec![wire::Modifier {
            name: "minecraft:sprinting".into(),
            amount: 0.5,
            operation: 2,
        }],
    };
    assert!(s.admit(event(
        7,
        1,
        wire::Packet::Attributes {
            entity: 42,
            values: vec![attribute]
        }
    )));
    for keep_attribute_modifiers in [true, false] {
        let old_epoch = s.terrain.dimension().unwrap().epoch;
        let spawn = s.bootstrap.as_ref().unwrap().world.spawn.clone();
        assert!(s.admit(Envelope {
            generation: 7,
            event: Event::Respawn {
                spawn,
                keep_attribute_modifiers,
                keep_metadata: false,
            }
        }));
        let attribute = &s.player_dynamics.attributes["minecraft:movement_speed"];
        assert_eq!(attribute.base, 0.1);
        assert_eq!(!attribute.modifiers.is_empty(), keep_attribute_modifiers);
        assert!(s.player_motion.is_none());
        assert!(!s.awaiting_correction_ack());
        assert_eq!(s.terrain.dimension().unwrap().epoch, old_epoch + 1);
        assert!(!s.admit(event(7, old_epoch, velocity(42))));
    }
}
