use super::*;
use crate::java::tests_support::state;
use java_protocol::{
    session::{Envelope, Event, dynamics},
    wire::Reader,
};

fn event(epoch: u64, packet: wire::Packet) -> Envelope {
    Envelope {
        generation: 7,
        event: Event::Entities {
            dimension_epoch: epoch,
            packet,
        },
    }
}
fn spawn() -> wire::Packet {
    wire::Packet::Spawn(wire::Spawn {
        id: 99,
        uuid: [1; 16],
        kind: "minecraft:pig",
        position: [-1. / 8192., 64., 0.12345],
        velocity: [0.; 3],
        pitch: 0.,
        yaw: 0.,
        head_yaw: 0.,
        data: 0,
    })
}
fn decoded(name: &str, bytes: &[u8]) -> wire::Packet {
    let mut r = Reader(bytes);
    let packet = wire::decode(name, &mut r).unwrap().unwrap();
    r.finish().unwrap();
    packet
}

#[test]
fn ordered_wire_deltas_reach_native_entity_authority_without_mutating_local_player() {
    let mut s = state();
    assert!(s.admit(event(1, spawn())));
    let identity = s.entities.get(99).unwrap().identity;
    assert!(s.admit(event(
        1,
        decoded(
            "ClientboundMoveEntityPosRotPacket",
            &[
                99, 5, 2, 0, 1, 0xff, 0xff, 0, 0, 4, 0, 2, 0, 0, 0, 1, 0x40, 0,
            ]
        )
    )));
    let e = s.entities.get(99).unwrap();
    assert_eq!(e.identity, identity);
    assert_eq!(
        e.motion.position,
        [3. / 4096., 64. - 1. / 4096., 507. / 4096.]
    );
    assert_eq!(e.delta_base, e.motion.position);
    assert_eq!(e.path.len(), 2);
    assert_eq!(e.path[0].tick_offset, 2);
    assert_eq!(e.path[1].tick_offset, 4);
    assert_eq!(e.motion.yaw, 90.);
    assert!(e.on_ground);
    assert!(s.player_motion.is_none());
    assert!(s.admit(Envelope {
        generation: 7,
        event: Event::Dynamics {
            dimension_epoch: 1,
            packet: dynamics::Packet::Velocity {
                entity: 99,
                velocity: [1., 2., 3.]
            },
        }
    }));
    assert_eq!(s.entities.get(99).unwrap().motion.velocity, [1., 2., 3.]);
    assert!(s.player_dynamics.unplaced_velocity.is_none());
}

#[test]
fn teleports_survive_coalesced_rotation_then_removal_and_id_reuse() {
    let mut s = state();
    s.admit(event(1, spawn()));
    let first = s.entities.get(99).unwrap().identity;
    assert!(s.admit(event(
        1,
        wire::Packet::Teleport {
            id: 99,
            position: [3., 70., 4.],
            velocity: [0.; 3],
            rotation: [90., 25.],
            relative: 0,
            on_ground: false,
        }
    )));
    let receipt = s.entities.get(99).unwrap().teleport_revision;
    assert!(s.admit(event(
        1,
        decoded("ClientboundMoveEntityRotPacket", &[99, 1, 0, 0])
    )));
    let e = s.entities.get(99).unwrap();
    assert_eq!(e.teleport_revision, receipt);
    assert_eq!(e.motion.position, [3., 70., 4.]);
    assert!(e.path.is_empty());
    assert!(s.admit(event(
        1,
        decoded("ClientboundRemoveEntitiesPacket", &[1, 99])
    )));
    assert!(!s.admit(event(
        1,
        decoded("ClientboundMoveEntityRotPacket", &[99, 1, 0, 0])
    )));
    assert!(s.entities.get(99).is_none());
    assert!(s.admit(event(1, spawn())));
    assert_ne!(s.entities.get(99).unwrap().identity, first);
}

#[test]
fn respawn_and_new_session_retire_entities_before_queued_old_updates() {
    let mut s = state();
    s.admit(event(1, spawn()));
    let spawn_facts = s.bootstrap.as_ref().unwrap().world.spawn.clone();
    assert!(s.admit(Envelope {
        generation: 7,
        event: Event::Respawn {
            spawn: spawn_facts,
            keep_attribute_modifiers: true,
            keep_metadata: true,
        }
    }));
    assert_eq!(s.entities.iter().count(), 0);
    assert!(s.entities.accepts(7, 2));
    assert!(!s.admit(event(1, spawn())));
    assert!(s.admit(event(2, spawn())));
    let before = s.entities.get(99).unwrap().identity;
    let mut retired = event(2, decoded("ClientboundRemoveEntitiesPacket", &[1, 99]));
    retired.generation = 6;
    assert!(!s.admit(retired));
    assert_eq!(s.entities.get(99).unwrap().identity, before);
    s.begin(8);
    assert_eq!(s.entities.iter().count(), 0);
}

#[test]
fn native_metadata_selects_registry_appearance_and_rejects_partial_or_stale_updates() {
    let mut s = state();
    assert!(s.admit(event(1, spawn())));
    let original = s.entities.get(99).unwrap();
    assert_eq!(
        original.visual.pig.as_ref().unwrap().name.as_ref(),
        "minecraft:temperate"
    );
    assert!(!original.visual.baby && !original.visual.invisible());
    // Independent target indexes and registry order: cold=0, pose crouching=5.
    let update = || {
        decoded(
            "ClientboundSetEntityDataPacket",
            &[99, 0, 0, 32, 16, 8, 1, 19, 28, 0, 255],
        )
    };
    assert!(!s.admit(event(9, update())));
    assert!(!s.entities.get(99).unwrap().visual.baby);
    assert!(s.admit(event(1, update())));
    let visual = &s.entities.get(99).unwrap().visual;
    assert!(visual.baby && visual.invisible());
    assert_eq!(
        visual.pig.as_ref().unwrap().model,
        world::java::entities::visual::PigModel::Cold
    );
    // A malformed accessor after a valid prefix cannot partially change admitted state.
    assert!(!s.admit(event(
        1,
        decoded(
            "ClientboundSetEntityDataPacket",
            &[99, 0, 0, 0, 19, 8, 1, 255]
        )
    )));
    assert!(s.entities.get(99).unwrap().visual.invisible());
    // An unsupported payload produces an explicit gap which later delta packets cannot erase.
    assert!(s.admit(event(
        1,
        decoded(
            "ClientboundSetEntityDataPacket",
            &[99, 0, 0, 0, 10, 17, 1, 255]
        )
    )));
    assert!(s.entities.get(99).unwrap().visual.incomplete.is_some());
    assert!(s.entities.get(99).unwrap().visual.invisible());
    assert!(s.admit(event(1, update())));
    assert!(s.entities.get(99).unwrap().visual.incomplete.is_some());
    assert!(s.admit(event(1, spawn())));
    assert!(s.entities.get(99).unwrap().visual.incomplete.is_none());
}

#[test]
fn local_metadata_stays_out_of_remote_store_and_obeys_respawn_retention() {
    for keep_metadata in [false, true] {
        let mut s = state();
        assert!(s.admit(event(
            1,
            decoded("ClientboundSetEntityDataPacket", &[42, 0, 0, 32, 255])
        )));
        assert!(s.player_visual.invisible());
        assert!(s.entities.get(42).is_none());
        let spawn = s.bootstrap.as_ref().unwrap().world.spawn.clone();
        assert!(s.admit(Envelope {
            generation: 7,
            event: Event::Respawn {
                spawn,
                keep_attribute_modifiers: false,
                keep_metadata
            }
        }));
        assert_eq!(s.player_visual.invisible(), keep_metadata);
        assert!(!s.admit(event(
            1,
            decoded("ClientboundSetEntityDataPacket", &[42, 0, 0, 0, 255])
        )));
        assert_eq!(s.player_visual.invisible(), keep_metadata);
    }
}
