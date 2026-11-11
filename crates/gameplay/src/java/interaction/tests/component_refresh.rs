//! Same-item authoritative refresh keeps the existing use countdown and hand owner.
use super::*;

#[test]
fn active_offhand_same_item_refreshes_components_before_release_without_restarting_use() {
    use crate::java::item_use::plan::{Active, Effects, End, Plan, ResultKind};
    let mut s = state();
    let mut stack = inventory::java::Stack {
        item: "minecraft:apple".into(),
        count: 2,
        added: Default::default(),
        removed: Default::default(),
    };
    let set = |s: &mut WorldState, stack: inventory::java::Stack| {
        s.inventory
            .admit(
                7,
                1,
                inventory::java::Update::PlayerSlot {
                    slot: 40,
                    stack: Some(stack),
                },
            )
            .unwrap();
    };
    set(&mut s, stack.clone());
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    s.player_visual.observe_use(3).unwrap();
    c.maintain_use(&s, 10, false);
    c.resume_server_use(
        &s,
        Plan {
            send: true,
            result: ResultKind::Consume(Some(Active {
                ticks: 32,
                end: End::Complete,
                effects: Effects {
                    can_sprint: false,
                    speed_multiplier: 0.2,
                },
            })),
        },
    );
    stack.count = 1;
    stack.added.insert(
        "minecraft:use_effects".into(),
        serde_json::json!({"can_sprint":true,"speed_multiplier":0.5}),
    );
    set(&mut s, stack);
    c.maintain_use(&s, 14, false);
    assert_eq!(c.using_hand(), Some(Hand::Off));
    assert_eq!(c.use_ticks(), Some(4));
    assert_eq!(
        c.use_effects(),
        Some(Effects {
            can_sprint: true,
            speed_multiplier: 0.5
        })
    );
    assert!(
        c.pending().is_none(),
        "component refresh allocates no action or sequence"
    );
    c.maintain_use(&s, 15, true);
    assert_eq!(c.pending().unwrap().kind, Kind::ReleaseUse);
    assert_eq!(c.pending().unwrap().sequence, 0);
    assert_eq!(c.use_ticks(), Some(5));
}

#[test]
fn diagnostics_distinguish_server_stop_from_inventory_replacement_without_sending_release() {
    use crate::java::item_use::plan::{Active, Effects, End, Plan, ResultKind};
    for (replace, reason) in [
        (false, "server_using_flag_cleared"),
        (true, "held_item_changed_or_unknown"),
    ] {
        let mut s = state();
        s.inventory
            .admit(
                7,
                1,
                inventory::java::Update::PlayerSlot {
                    slot: 0,
                    stack: Some(inventory::java::Stack {
                        item: "minecraft:apple".into(),
                        count: 2,
                        added: Default::default(),
                        removed: Default::default(),
                    }),
                },
            )
            .unwrap();
        let mut c = Controller::default();
        begin(&mut c, &s, Duration::ZERO);
        s.player_visual.observe_use(1).unwrap();
        c.maintain_use(&s, 0, false);
        c.resume_server_use(
            &s,
            Plan {
                send: true,
                result: ResultKind::Consume(Some(Active {
                    ticks: 32,
                    end: End::Complete,
                    effects: Effects {
                        can_sprint: false,
                        speed_multiplier: 0.2,
                    },
                })),
            },
        );
        assert!(c.using_item());
        if replace {
            s.inventory
                .admit(
                    7,
                    1,
                    inventory::java::Update::PlayerSlot {
                        slot: 0,
                        stack: None,
                    },
                )
                .unwrap();
        } else {
            s.player_visual.observe_use(0).unwrap();
        }
        c.maintain_use(&s, 1, false);
        assert!(!c.using_item());
        assert_eq!(c.use_stop_reason(), Some(reason));
        assert!(
            c.pending().is_none(),
            "observing the server never invents a release packet"
        );
    }
}
