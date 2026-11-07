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
        lifetime: 1,
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
fn remote_players_bind_profiles_and_clothing_without_reusing_old_entity_lifetimes() {
    use java_protocol::{session::roster, target};
    let mut s = state();
    let add = |name: &str| Envelope {
        generation: 7,
        event: Event::Roster(roster::Packet::Update(vec![roster::Update {
            uuid: [1; 16],
            profile: Some(roster::Profile {
                name: name.into(),
                properties: vec![],
            }),
            ..Default::default()
        }])),
    };
    let player = || {
        let wire::Packet::Spawn(mut spawn) = spawn() else {
            unreachable!()
        };
        spawn.kind = target::ENTITY_PLAYER;
        wire::Packet::Spawn(spawn)
    };
    // Missing identity must not fabricate a profile or choose an unrelated player's appearance.
    assert!(s.admit(event(1, player())));
    assert!(s.entities.get(99).unwrap().visual.player.is_none());
    assert!(s.entities.get(99).unwrap().presentation.is_none());
    assert!(s.admit(add("First")));
    assert!(s.admit(event(1, player())));
    let first = s.entities.get(99).unwrap().identity;
    assert!(s.entities.get(99).unwrap().presentation.is_some());
    assert_eq!(
        s.entities
            .get(99)
            .unwrap()
            .visual
            .player
            .as_ref()
            .unwrap()
            .name
            .as_ref(),
        "First"
    );
    // Target accessor 16, byte serializer 0: jacket and hat only.
    assert!(s.admit(event(
        1,
        decoded("ClientboundSetEntityDataPacket", &[99, 16, 0, 0x42, 255])
    )));
    let layers = s.entities.get(99).unwrap().visual.player_layers;
    assert!(layers.hat && layers.jacket);
    assert!(
        !layers.left_sleeve
            && !layers.right_sleeve
            && !layers.left_pants
            && !layers.right_pants
            && !layers.cape
    );
    assert!(s.admit(Envelope {
        generation: 7,
        event: Event::Roster(roster::Packet::Remove(vec![[1; 16]]))
    }));
    assert!(s.players.get(&[1; 16]).is_none());
    // Native entities cache their immutable profile even when it leaves the tab list.
    assert_eq!(
        s.entities
            .get(99)
            .unwrap()
            .visual
            .player
            .as_ref()
            .unwrap()
            .name
            .as_ref(),
        "First"
    );
    s.admit(add("Second"));
    assert!(s.admit(event(1, player())));
    let replacement = s.entities.get(99).unwrap();
    assert_ne!(replacement.identity, first);
    assert_eq!(
        replacement.visual.player.as_ref().unwrap().name.as_ref(),
        "Second"
    );
    assert_eq!(replacement.visual.player_layers, Default::default());
    s.begin(8);
    assert_eq!(s.players.iter().count(), 0);
    assert_eq!(s.entities.iter().count(), 0);
}

#[test]
fn far_position_sync_retires_old_visual_targets_and_rebases_wire_deltas() {
    let mut s = state();
    s.admit(event(1, spawn()));
    let sync = |position| wire::Packet::Position {
        id: 99,
        path: wire::Position::Linear(position),
        rotation: [90., 0.],
        on_ground: true,
    };
    assert!(s.admit(event(1, sync([1., 64., 0.]))));
    assert!(s.admit(event(1, sync([100., 64., 0.]))));
    let e = s.entities.get(99).unwrap();
    // Pinned ClientPacketListener.handleEntityPositionSync snaps beyond 64 blocks.
    assert_eq!(
        e.presentation.as_ref().unwrap().track.current.position,
        [100., 64., 0.]
    );
    assert_eq!(e.presentation.as_ref().unwrap().track.pending_steps(), 0);
    // Independent relative packet: id 99, dx 4096, dy/dz zero, on-ground true.
    assert!(s.admit(event(
        1,
        decoded(
            "ClientboundMoveEntityPosPacket",
            &[99, 1, 0x10, 0, 0, 0, 0, 0]
        )
    )));
    assert_eq!(s.entities.get(99).unwrap().motion.position, [101., 64., 0.]);
}

#[test]
fn native_motion_packets_seed_and_queue_presentation_but_metadata_and_velocity_do_not() {
    let mut s = state();
    s.admit(event(1, spawn()));
    let position = s.entities.get(99).unwrap().motion.position;
    let packet = |position| wire::Packet::Position {
        id: 99,
        path: wire::Position::Linear(position),
        rotation: [0., 0.],
        on_ground: true,
    };
    assert_eq!(s.entities.pending_presentation_steps(), 0);
    assert!(s.admit(event(1, packet(position))));
    assert_eq!(
        s.entities.pending_presentation_steps(),
        1,
        "unchanged first target seeds the native handler"
    );
    for _ in 0..4 {
        s.entities.tick_presentation();
    }
    for x in [1., 2.] {
        assert!(s.admit(event(1, packet([x, 64., position[2]]))));
    }
    assert_eq!(s.entities.pending_presentation_steps(), 2);
    assert!(s.admit(event(
        1,
        decoded("ClientboundSetEntityDataPacket", &[99, 16, 8, 1, 255])
    )));
    assert!(s.admit(Envelope {
        generation: 7,
        event: Event::Dynamics {
            dimension_epoch: 1,
            packet: dynamics::Packet::Velocity {
                entity: 99,
                velocity: [1.; 3]
            }
        }
    }));
    assert_eq!(s.entities.pending_presentation_steps(), 2);
    assert!(!s.admit(event(2, packet([99.; 3]))));
    s.entities.tick_presentation();
    let e = s.entities.get(99).unwrap();
    assert_eq!(e.motion.position, [2., 64., position[2]]);
    assert!(e.presentation.as_ref().unwrap().track.current.position[0] < 1.);
    assert_eq!(e.motion.velocity, [1.; 3]);
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
    // The old particle payload is truncated now that particles have a codec.
    assert!(
        wire::decode(
            "ClientboundSetEntityDataPacket",
            &mut Reader(&[99, 0, 0, 0, 10, 17, 1, 255])
        )
        .is_err()
    );
    // RESOLVABLE_PROFILE is still deferred; no valid flags prefix may be admitted.
    assert!(s.admit(event(
        1,
        decoded(
            "ClientboundSetEntityDataPacket",
            &[99, 0, 0, 0, 10, 41, 1, 255]
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

#[test]
fn local_use_flags_and_completion_are_scoped_authority_without_inventory_prediction() {
    let mut s = state();
    let inventory = s.inventory.revision;
    assert!(s.admit(event(
        1,
        decoded("ClientboundSetEntityDataPacket", &[42, 8, 0, 1, 255])
    )));
    assert_eq!(
        (s.player_visual.use_flags, s.player_visual.use_revision),
        (1, 1)
    );
    assert!(s.entities.get(42).is_none());
    assert!(s.admit(event(
        1,
        decoded("ClientboundEntityEventPacket", &[0, 0, 0, 42, 9])
    )));
    assert_eq!(
        (s.player_visual.use_flags, s.player_visual.use_revision),
        (0, 2)
    );
    assert_eq!(
        s.inventory.revision, inventory,
        "use completion is not a slot-content acknowledgement"
    );
    assert!(!s.admit(event(
        2,
        decoded("ClientboundSetEntityDataPacket", &[42, 8, 0, 3, 255])
    )));
    assert_eq!(
        (s.player_visual.use_flags, s.player_visual.use_revision),
        (0, 2)
    );
    assert!(s.admit(event(
        1,
        decoded("ClientboundSetEntityDataPacket", &[42, 8, 0, 3, 255])
    )));
    assert_eq!(
        s.player_visual.use_flags, 3,
        "offhand bit is retained distinctly"
    );
    s.begin(8);
    assert_eq!(
        (s.player_visual.use_flags, s.player_visual.use_revision),
        (0, 0)
    );
}

#[test]
fn equipment_is_atomic_incarnation_scoped_and_separate_from_personal_inventory() {
    use java_protocol::session::entities::equipment;
    let mut s = state();
    assert!(s.admit(event(1, spawn())));
    let identity = s.entities.get(99).unwrap().identity;
    let inventory_before = serde_json::to_value(&s.inventory).unwrap();
    let packet = |lifetime| {
        wire::Packet::Equipment(equipment::Packet {
            id: 99,
            lifetime,
            slots: [
                (
                    0,
                    Some(java_protocol::session::inventory::Stack {
                        item: 1,
                        count: 3,
                        added: Default::default(),
                        removed: Default::default(),
                    }),
                ),
                (5, None),
            ]
            .into(),
        })
    };
    assert!(!s.admit(event(1, packet(0))));
    assert!(s.admit(event(1, packet(1))));
    assert_eq!(
        s.equipment.get(identity).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        3
    );
    assert_eq!(
        serde_json::to_value(&s.inventory).unwrap(),
        inventory_before
    );
    assert!(!s.admit(event(2, packet(1))));
    assert!(s.admit(event(1, wire::Packet::Remove(vec![99]))));
    assert!(s.equipment.get(identity).is_none());
    let wire::Packet::Spawn(mut replacement) = spawn() else {
        unreachable!()
    };
    replacement.lifetime = 2;
    assert!(s.admit(event(1, wire::Packet::Spawn(replacement))));
    assert!(!s.admit(event(1, packet(1))));
    assert!(s.admit(event(1, packet(2))));
    let identity2 = s.entities.get(99).unwrap().identity;
    assert_ne!(identity, identity2);
    assert!(s.equipment.get(identity2).is_some());
    s.begin(8);
    assert!(s.equipment.get(identity2).is_none());
}

mod ground;

mod swing;

mod villager;

#[test]
fn player_potion_aura_metadata_keeps_body_layers_and_identity_visible() {
    use java_protocol::{session::roster, target};
    let mut s = state();
    assert!(s.admit(Envelope {
        generation: 7,
        event: Event::Roster(roster::Packet::Update(vec![roster::Update {
            uuid: [1; 16],
            profile: Some(roster::Profile {
                name: "AuraPlayer".into(),
                properties: vec![]
            }),
            ..Default::default()
        }]))
    }));
    let wire::Packet::Spawn(mut p) = spawn() else {
        panic!()
    };
    p.kind = target::ENTITY_PLAYER;
    assert!(s.admit(event(1, wire::Packet::Spawn(p))));
    let identity = s.entities.get(99).unwrap().identity;
    let particle = target::PARTICLES
        .iter()
        .position(|p| p.name == "minecraft:entity_effect")
        .unwrap() as i32;
    let effects = target::METADATA_PLAYER
        .iter()
        .find(|m| m.name == "DATA_EFFECT_PARTICLES")
        .unwrap();
    let layers = target::METADATA_PLAYER
        .iter()
        .find(|m| m.name == "DATA_PLAYER_MODE_CUSTOMISATION")
        .unwrap();
    let mut bytes = vec![99, 0, 0, 0, effects.index];
    java_protocol::codec::write_varint(effects.serializer as i32, &mut bytes);
    bytes.push(1);
    java_protocol::codec::write_varint(particle, &mut bytes);
    bytes.extend(0xff88cc55u32.to_be_bytes());
    bytes.extend([layers.index, layers.serializer as u8, 0x42, 255]);
    assert!(s.admit(event(1, decoded("ClientboundSetEntityDataPacket", &bytes))));
    let e = s.entities.get(99).unwrap();
    assert_eq!(e.identity, identity);
    assert!(e.visual.incomplete.is_none());
    assert!(e.presentation.is_some());
    assert!(!e.visual.invisible());
    assert!(e.visual.player_layers.hat);
    assert!(e.visual.player_layers.jacket);
}

#[test]
fn armor_stand_name_metadata_survives_invisibility_and_updates_without_body_support() {
    use java_protocol::{
        session::entities::metadata::{Entry, Packet, Value},
        target,
    };
    let mut s = state();
    let wire::Packet::Spawn(mut a) = spawn() else {
        unreachable!()
    };
    a.kind = target::ENTITY_ARMOR_STAND;
    assert!(s.admit(event(1, wire::Packet::Spawn(a))));
    let entry = |name, value| {
        let spec = target::METADATA_ARMOR_STAND
            .iter()
            .find(|s| s.name == name)
            .unwrap();
        Entry {
            index: spec.index,
            serializer: spec.serializer,
            value,
        }
    };
    let packet = || {
        wire::Packet::Metadata(Packet {
            id: 99,
            unsupported: None,
            entries: vec![
                entry("DATA_SHARED_FLAGS_ID", Value::Byte(32)),
                entry(
                    "DATA_CUSTOM_NAME",
                    Value::Component(Some(java_protocol::nbt::Value::String("SHOP".into()))),
                ),
                entry("DATA_CUSTOM_NAME_VISIBLE", Value::Bool(true)),
                entry("DATA_CLIENT_FLAGS", Value::Byte(16)),
            ],
        })
    };
    assert!(s.admit(event(1, packet())));
    let e = s.entities.get(99).unwrap();
    assert!(e.visual.invisible() && e.visual.custom_name_visible);
    assert_eq!(e.visual.armor_stand_flags, Some(16));
    assert!(e.presentation.is_some() && e.visual.incomplete.is_none());
    assert_eq!(
        e.visual.custom_name.as_deref(),
        Some(&world::NbtValue::String("SHOP".into()))
    );
    assert!(!s.admit(event(9, packet())));
    assert!(s.admit(event(
        1,
        wire::Packet::Metadata(Packet {
            id: 99,
            unsupported: None,
            entries: vec![entry("DATA_CUSTOM_NAME", Value::Component(None))]
        })
    )));
    assert!(s.entities.get(99).unwrap().visual.custom_name.is_none());
}
#[test]
fn server_hurt_feedback_is_visual_only_lifetime_fenced_and_expires() {
    let mut s = state();
    assert!(s.admit(event(1, spawn())));
    s.entities
        .update((7, 1), 99, |e| {
            e.visual.health = Some(20.);
            Ok(())
        })
        .unwrap();
    let hurt = |lifetime| wire::Packet::Hurt { lifetime, id: 99 };
    assert!(!s.admit(event(1, hurt(2))));
    assert!(!s.admit(event(9, hurt(1))));
    assert_eq!(s.entities.get(99).unwrap().visual.hurt_ticks, 0);
    assert!(s.admit(event(1, hurt(1))));
    assert_eq!(s.entities.get(99).unwrap().visual.hurt_ticks, 10);
    assert_eq!(
        s.entities.get(99).unwrap().visual.health,
        Some(20.),
        "no damage award from a visual notification"
    );
    for _ in 0..10 {
        s.entities.tick_presentation();
    }
    assert_eq!(s.entities.get(99).unwrap().visual.hurt_ticks, 0);
    assert!(s.admit(event(1, hurt(1))));
    assert!(s.admit(event(1, wire::Packet::Remove(vec![99]))));
    let wire::Packet::Spawn(mut replacement) = spawn() else {
        unreachable!()
    };
    replacement.lifetime = 2;
    assert!(s.admit(event(1, wire::Packet::Spawn(replacement))));
    assert_eq!(s.entities.get(99).unwrap().visual.hurt_ticks, 0);
    assert!(!s.admit(event(1, hurt(1))));
    assert!(s.admit(event(1, hurt(2))));
}
