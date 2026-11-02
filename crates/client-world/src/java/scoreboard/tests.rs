use super::*;
fn objective(s: &mut State, name: &str, method: u8) {
    s.admit(wire::Packet::Objective {
        name: name.into(),
        method,
        display: Some((java_protocol::nbt::Value::String(name.into()), 0, None)),
    });
}
fn score(s: &mut State, name: &str, owner: &str, n: i32) {
    s.admit(wire::Packet::Score {
        owner: owner.into(),
        objective: name.into(),
        score: n,
        display: None,
        format: None,
    });
}
#[test]
fn sidebar_selection_order_and_removal_are_authoritative() {
    let mut s = State::default();
    objective(&mut s, "lobby", 0);
    objective(&mut s, "red", 0);
    s.admit(wire::Packet::Display {
        slot: 1,
        objective: "lobby".into(),
    });
    for (name, n) in [("zeta", 7), ("Alpha", 7), ("#hidden", 999), ("last", -1)] {
        score(&mut s, "lobby", name, n);
    }
    assert_eq!(
        s.sidebar(None, 15)
            .unwrap()
            .1
            .iter()
            .map(|r| r.owner.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "zeta", "last"]
    );
    score(&mut s, "red", "Red team", 1);
    s.admit(wire::Packet::Display {
        slot: 15,
        objective: "red".into(),
    });
    assert_eq!(s.sidebar(Some('c'), 15).unwrap().1[0].owner, "Red team");
    s.admit(wire::Packet::Reset {
        owner: "Alpha".into(),
        objective: Some("red".into()),
    });
    assert_eq!(s.sidebar(None, 15).unwrap().1.len(), 3);
    s.admit(wire::Packet::Reset {
        owner: "Alpha".into(),
        objective: None,
    });
    assert_eq!(s.sidebar(None, 15).unwrap().1.len(), 2);
    objective(&mut s, "red", 1);
    assert_eq!(s.sidebar(Some('c'), 15).unwrap().1[0].owner, "zeta");
    let r = s.revision();
    s.clear();
    assert!(s.sidebar(None, 15).is_none());
    assert_ne!(s.revision(), r);
}
#[test]
fn scores_survive_title_update_and_only_fifteen_visible_rows_are_selected() {
    let mut s = State::default();
    objective(&mut s, "lobby", 0);
    for i in 0..20 {
        score(&mut s, "lobby", &format!("{i:02}"), i);
    }
    s.admit(wire::Packet::Display {
        slot: 1,
        objective: "lobby".into(),
    });
    objective(&mut s, "lobby", 2);
    let (_, rows) = s.sidebar(None, 15).unwrap();
    assert_eq!(rows.len(), 15);
    assert_eq!(rows[0].score, 19);
    assert_eq!(rows[14].score, 5);
    objective(&mut s, "lobby", 1);
    objective(&mut s, "lobby", 0);
    s.admit(wire::Packet::Display {
        slot: 1,
        objective: "lobby".into(),
    });
    assert!(s.sidebar(None, 15).unwrap().1.is_empty());
}
#[test]
fn old_session_events_cannot_replace_sidebar_and_login_retires_old_objectives() {
    use java_protocol::session::{Envelope, Event};
    let mut state = super::super::tests_support::state();
    assert!(!state.admit(Envelope {
        generation: 6,
        event: Event::Scoreboard(wire::Packet::Objective {
            name: "stale".into(),
            method: 0,
            display: Some((java_protocol::nbt::Value::String("OLD".into()), 0, None))
        })
    }));
    assert!(state.scoreboard.objectives.is_empty());
    objective(&mut state.scoreboard, "lobby", 0);
    score(&mut state.scoreboard, "lobby", "Alice", 9);
    state.scoreboard.admit(wire::Packet::Display {
        slot: 1,
        objective: "lobby".into(),
    });
    assert!(state.scoreboard.sidebar(None, 15).is_some());
    let world = state.bootstrap.as_ref().unwrap().world.clone();
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::WorldLogin(Box::new(world))
    }));
    assert!(state.scoreboard.sidebar(None, 15).is_none());
    assert!(state.scoreboard.scores.is_empty());
}
