//! Real entity lifetime, effect admission and render-tick integration, without socket or GPU mocks.
use super::*;
#[test]
fn swing_lifetime_effects_and_reconnect_do_not_change_player_simulation() {
    let mut s = state();
    assert!(s.admit(event(1, spawn())));
    let swing = |lifetime| wire::Packet::Swing {
        lifetime,
        id: 99,
        offhand: true,
        kind: 1,
        duration: 6,
    };
    assert!(!s.admit(event(1, swing(0))));
    assert!(s.admit(event(1, swing(1))));
    for _ in 0..3 {
        s.entities.tick_presentation();
    }
    let sample = s.entities.get(99).unwrap().visual.swing.sample(1.).unwrap();
    assert!(sample.offhand);
    assert!((sample.progress - 1. / 3.).abs() < 0.000001);
    assert!(s.admit(event(1, wire::Packet::Remove(vec![99]))));
    let wire::Packet::Spawn(mut fresh) = spawn() else {
        panic!()
    };
    fresh.lifetime = 2;
    assert!(s.admit(event(1, wire::Packet::Spawn(fresh))));
    assert!(!s.admit(event(1, swing(1))));
    assert!(
        s.entities
            .get(99)
            .unwrap()
            .visual
            .swing
            .sample(1.)
            .is_none()
    );
    let mut stale = event(1, swing(2));
    stale.generation = 6;
    assert!(!s.admit(stale));
    assert!(!s.admit(event(2, swing(2))));
}

#[test]
fn remote_effect_duration_changes_only_its_lifetime_scoped_swing() {
    let mut s = state();
    assert!(s.admit(event(1, spawn())));
    let effect = |value| Envelope {
        generation: 7,
        event: Event::Dynamics {
            dimension_epoch: 1,
            packet: dynamics::Packet::Effect {
                entity: 99,
                effect: "minecraft:haste",
                value,
            },
        },
    };
    assert!(s.admit(effect(Some(dynamics::Effect {
        amplifier: 1,
        duration: 2,
        flags: 0
    }))));
    assert!(s.player_dynamics.effects.is_empty());
    assert_eq!(
        domain::animation::swing::modified_duration(&s.entities.get(99).unwrap().visual.effects, 6),
        4
    );
    s.entities.tick_presentation();
    assert_eq!(
        s.entities.get(99).unwrap().visual.effects["minecraft:haste"].duration,
        1
    );
    s.entities.tick_presentation();
    assert!(s.entities.get(99).unwrap().visual.effects.is_empty());
    assert!(s.admit(effect(Some(dynamics::Effect {
        amplifier: 2,
        duration: -1,
        flags: 0
    }))));
    for _ in 0..10 {
        s.entities.tick_presentation();
    }
    assert_eq!(
        s.entities.get(99).unwrap().visual.effects["minecraft:haste"].duration,
        -1
    );
    assert!(s.admit(effect(None)));
    assert!(s.entities.get(99).unwrap().visual.effects.is_empty());
    assert!(s.admit(event(1, wire::Packet::Remove(vec![99]))));
    assert!(!s.admit(effect(Some(dynamics::Effect {
        amplifier: 2,
        duration: 20,
        flags: 0
    }))));
    let wire::Packet::Spawn(mut item) = spawn() else {
        panic!()
    };
    item.kind = java_protocol::target::ENTITY_ITEM;
    item.lifetime = 2;
    assert!(s.admit(event(1, wire::Packet::Spawn(item))));
    assert!(!s.admit(event(
        1,
        wire::Packet::Swing {
            lifetime: 2,
            id: 99,
            offhand: false,
            kind: 1,
            duration: 6
        }
    )));
}
