use super::*;
use world::java::{
    entities::{Entities, Spawn, visual::Visual},
    player::Motion,
    roster::{DefaultSkin, Roster, teams},
};
fn spawn(entities: &mut Entities, name: &str, position: [f64; 3]) {
    entities
        .spawn(
            (7, 1),
            Spawn {
                network_id: 10,
                uuid: [1; 16],
                kind: "minecraft:player".into(),
                motion: Motion {
                    position,
                    ..Default::default()
                },
                head_yaw: 0.,
                object_data: 0,
                visual: Visual {
                    player: Some(Arc::new(Profile {
                        name: name.into(),
                        properties: Arc::from([]),
                        default_skin: DefaultSkin {
                            texture: "fixture:skin".into(),
                            slim: false,
                        },
                    })),
                    ..Default::default()
                },
                interpolation_interval: Some(3),
            },
        )
        .unwrap();
}
fn view() -> NametagView {
    NametagView {
        camera_position: glam::Vec3::ZERO,
        camera_forward: glam::Vec3::NEG_Z,
    }
}
fn team(visibility: teams::Visibility) -> teams::Update {
    teams::Update {
        team: "red".into(),
        action: teams::Action::Create,
        members: vec!["Alice".into(), "Viewer".into()],
        parameters: Some(Parameters {
            display: world::NbtValue::String("Red".into()),
            prefix: world::NbtValue::String("[R] ".into()),
            suffix: world::NbtValue::String("".into()),
            visibility,
            collision: teams::Collision::Always,
            color: Some('c'),
            options: 2,
        }),
    }
}
#[test]
fn names_reach_world_anchors_and_team_changes_replace_cached_text_and_visibility() {
    let mut e = Entities::default();
    e.begin(7, 1);
    spawn(&mut e, "Alice", [0., 0., -5.]);
    let mut roster = Roster::new(7);
    let mut c = Cache::default();
    let tags = c.project_players(&e, &roster, "Viewer", None, view(), 0.);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].lines[0].as_ref(), "§rAlice");
    assert_eq!(tags[0].position, glam::Vec3::new(0., 2.3, -5.));
    let retained = Arc::clone(&c.entries.values().next().unwrap().lines.as_ref().unwrap());
    let buffer = tags.as_ptr();
    c.recycle_anchors(tags);
    let repeated = c.project_players(&e, &roster, "Viewer", None, view(), 0.);
    assert_eq!(buffer, repeated.as_ptr());
    c.recycle_anchors(repeated);
    assert!(Arc::ptr_eq(
        &retained,
        c.entries.values().next().unwrap().lines.as_ref().unwrap()
    ));
    roster.update_team(7, team(teams::Visibility::Always));
    let tags = c.project_players(&e, &roster, "Viewer", None, view(), 0.);
    assert!(tags[0].lines[0].contains("§c[R] "));
    roster.update_team(7, team(teams::Visibility::Never));
    assert!(
        c.project_players(&e, &roster, "Viewer", None, view(), 0.)
            .is_empty()
    );
    assert!(c.entries.is_empty());
}
#[test]
fn invisibility_sneak_range_and_reused_network_ids_do_not_leak_labels() {
    let mut e = Entities::default();
    e.begin(7, 1);
    spawn(&mut e, "Alice", [0., 0., -5.]);
    let mut c = Cache::default();
    let mut roster = Roster::new(7);
    e.update((7, 1), 10, |e| {
        e.visual.flags = 0x20;
        Ok(())
    })
    .unwrap();
    assert!(
        c.project_players(&e, &roster, "Viewer", None, view(), 0.)
            .is_empty()
    );
    roster.update_team(7, team(teams::Visibility::Always));
    assert_eq!(
        c.project_players(&e, &roster, "Viewer", None, view(), 0.)
            .len(),
        1
    );
    spawn(&mut e, "New", [0., 0., -40.]);
    e.update((7, 1), 10, |e| {
        e.visual.flags = 2;
        Ok(())
    })
    .unwrap();
    assert!(
        c.project_players(&e, &roster, "Viewer", None, view(), 0.)
            .is_empty()
    );
    e.update((7, 1), 10, |e| {
        e.visual.flags = 0;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        c.project_players(&e, &roster, "Viewer", None, view(), 0.)[0].lines[0].as_ref(),
        "§rNew"
    );
    e.remove((7, 1), &[10]).unwrap();
    assert!(
        c.project_players(&e, &roster, "Viewer", None, view(), 0.)
            .is_empty()
    );
    assert!(c.entries.is_empty());
    spawn(&mut e, "Reconnected", [0., 0., -5.]);
    c.project_players(&e, &roster, "Viewer", None, view(), 0.);
    c.clear();
    assert!(c.entries.is_empty());
}

#[test]
fn native_crouching_attachment_does_not_receive_a_second_legacy_sneak_drop() {
    let mut e = Entities::default();
    e.begin(7, 1);
    spawn(&mut e, "Alice", [0., 0., -5.]);
    e.update((7, 1), 10, |e| {
        e.visual.flags = 2;
        e.visual.pose = "crouching".into();
        Ok(())
    })
    .unwrap();
    let mut c = Cache::default();
    let tags = c.project_players(&e, &Roster::new(7), "Viewer", None, view(), 0.);
    assert_eq!(tags[0].position, glam::Vec3::new(0., 2., -5.));
    assert!(tags[0].depth_tested);
    assert!(!tags[0].sneaking); // The private shared flag controls the older camera-space drop.
    assert_eq!(tags[0].text_alpha, 0.5);
}
