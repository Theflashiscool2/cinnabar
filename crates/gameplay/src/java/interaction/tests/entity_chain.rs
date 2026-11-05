use super::*;
use crate::java::item_use::plan::{Plan, ResultKind};
fn entity(state: &mut WorldState, lifetime: u64) -> EntityTarget {
    use world::java::{
        entities::{Spawn, visual::Visual},
        player::Motion,
    };
    state.entities.begin(7, 1);
    state
        .entities
        .spawn(
            (7, 1),
            Spawn {
                transport_lifetime: lifetime,
                network_id: 42,
                uuid: [1; 16],
                kind: "minecraft:player".into(),
                motion: Motion::default(),
                head_yaw: 0.,
                object_data: 0,
                visual: Visual::default(),
                interpolation_interval: Some(3),
            },
        )
        .unwrap();
    EntityTarget {
        network_id: 42,
        identity: state.entities.get(42).unwrap().identity,
        lifetime,
        location: [0.1, 1.5, -0.3],
        secondary: true,
    }
}
fn air(hand: Hand) -> UseStep {
    UseStep::air(
        [90., 20.],
        hand,
        Plan {
            send: true,
            result: ResultKind::Pass,
        },
        &None,
    )
    .unwrap()
}
#[test]
fn entity_hands_share_order_but_do_not_advance_block_prediction_sequence() {
    let mut s = state();
    let e = entity(&mut s, 8);
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.use_chain(
        &s,
        press(),
        Ok(vec![
            UseStep::entity(e, Hand::Main),
            air(Hand::Main),
            UseStep::entity(e, Hand::Off),
            air(Hand::Off),
        ]),
    );
    for (kind, hand, sequence) in [
        (Kind::UseEntity, Hand::Main, 0),
        (Kind::UseAir, Hand::Main, 1),
        (Kind::UseEntity, Hand::Off, 0),
        (Kind::UseAir, Hand::Off, 2),
    ] {
        let r = c.pending().unwrap();
        assert_eq!((r.kind, r.hand, r.sequence), (kind, hand, sequence));
        assert!(c.validate_use_entity(&s));
        c.queued();
    }
    assert_eq!(c.entity_uses_queued, 2);
    assert!(c.pending().is_none());
    assert_eq!(c.queued_actions, 4);
}
#[test]
fn removal_or_reused_id_retires_unsent_air_and_offhand_of_the_same_click() {
    for replace in [false, true] {
        let mut s = state();
        let e = entity(&mut s, 8);
        let mut c = Controller::default();
        begin(&mut c, &s, Duration::ZERO);
        c.use_chain(
            &s,
            press(),
            Ok(vec![
                UseStep::entity(e, Hand::Main),
                air(Hand::Main),
                UseStep::entity(e, Hand::Off),
            ]),
        );
        c.queued();
        assert_eq!(c.pending().unwrap().kind, Kind::UseAir);
        s.entities.remove((7, 1), &[42]).unwrap();
        if replace {
            entity(&mut s, 9);
        }
        assert!(!c.validate_use_entity(&s));
        assert!(c.pending().is_none());
        assert!(c.use_steps.is_empty());
        assert_eq!(c.queued_actions, 1);
        c.use_target(&s, press(), Ok(Some(Target::Air([0., 0.]))));
        assert_eq!(c.pending().unwrap().sequence, 2);
        assert!(c.validate_use_entity(&s));
    }
}
#[test]
fn authority_release_retires_both_entity_and_item_stages() {
    let mut s = state();
    let e = entity(&mut s, 8);
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.use_chain(
        &s,
        press(),
        Ok(vec![UseStep::entity(e, Hand::Main), air(Hand::Main)]),
    );
    c.begin_frame(
        &s,
        Duration::ZERO,
        Some(InputAuthorityGeneration::INITIAL),
        true,
    );
    assert!(c.pending().is_none());
    assert!(c.use_steps.is_empty());
    assert!(c.use_entity.is_none());
}
