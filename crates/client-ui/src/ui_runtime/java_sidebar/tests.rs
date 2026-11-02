use super::*;
use client_world::java::scoreboard::{Objective, Row};
fn literal(s: &str) -> NbtValue {
    NbtValue::String(s.into())
}
#[test]
fn java_sidebar_formats_rows_and_number_overrides_without_replacing_scores() {
    let objective = Objective {
        display: literal("LOBBY"),
        render: 0,
        format: Some(NumberFormat::Fixed(literal("*"))),
    };
    let mut row = Row {
        owner: "Alice".into(),
        score: 42,
        display: Some(literal("Wins")),
        format: None,
    };
    let mut players = world::java::roster::Roster::new(7);
    use world::java::roster::teams::{Action, Collision, Parameters, Update, Visibility};
    assert!(players.update_team(
        7,
        Update {
            team: "a".into(),
            action: Action::Create,
            members: vec!["Alice".into()],
            parameters: Some(Parameters {
                display: literal("A"),
                prefix: literal("[A] "),
                suffix: literal(" !"),
                color: Some('a'),
                visibility: Visibility::Always,
                collision: Collision::Always,
                options: 0
            })
        }
    ));
    let p = project(&objective, &[&row], &players).unwrap();
    assert_eq!(&*p.title, "§rLOBBY");
    assert_eq!(&*p.rows[0].label, "§r§a[A] §r§aWins§r§a !");
    assert_eq!(p.rows[0].value, PresentedScoreValue::Text("§r*".into()));
    row.format = Some(NumberFormat::Blank);
    assert_eq!(
        project(&objective, &[&row], &players).unwrap().rows[0].value,
        PresentedScoreValue::Text("".into())
    );
    row.format = None;
    let objective = Objective {
        format: None,
        ..objective
    };
    assert_eq!(
        project(&objective, &[&row], &players).unwrap().rows[0].value,
        PresentedScoreValue::Text("§r§c42".into())
    );
    assert_eq!(row.score, 42);
}
#[test]
fn native_sidebar_rejects_retired_session_and_clears_without_touching_bedrock_store() {
    let mut ui = UiRuntime::new(7);
    let mut state = WorldState::default();
    state.begin(6);
    let before = ui.scoreboards().revision();
    assert!(!ui.publish_java_sidebar(&state));
    assert_eq!(ui.scoreboards().revision(), before);
    assert!(ui.session.java_sidebar.projection.is_none());
}

#[test]
fn styled_numbers_use_the_received_style_and_unsupported_components_are_explicit() {
    let players = world::java::roster::Roster::new(7);
    let mut style = world::NbtCompound::default();
    style.insert("color", literal("gold"));
    style.insert("bold", NbtValue::Byte(1));
    let objective = Objective {
        display: literal("Board"),
        render: 0,
        format: Some(NumberFormat::Styled(NbtValue::Compound(style))),
    };
    let row = Row {
        owner: "名".into(),
        score: -4,
        display: None,
        format: None,
    };
    let result = project(&objective, &[&row], &players).unwrap();
    assert_eq!(&*result.rows[0].label, "§r名");
    assert_eq!(
        result.rows[0].value,
        PresentedScoreValue::Text("§r§6§l-4".into())
    );
    let mut translated = world::NbtCompound::default();
    translated.insert("translate", literal("not.a.literal"));
    let objective = Objective {
        display: NbtValue::Compound(translated),
        ..objective
    };
    assert!(project(&objective, &[&row], &players).is_err());
}
