use super::*;
fn add(uuid: [u8; 16], name: &str) -> Update {
    Update {
        uuid,
        profile: Some(Arc::new(Profile {
            name: name.into(),
            properties: Arc::from([]),
            default_skin: DefaultSkin {
                texture: "fixture:skin".into(),
                slim: false,
            },
        })),
        ..Default::default()
    }
}

#[test]
fn native_two_pass_additions_keep_first_profile_and_apply_all_updates() {
    let mut roster = Roster::new(7);
    let id = [1; 16];
    assert!(roster.update(
        7,
        vec![
            Update {
                uuid: id,
                latency: Some(42),
                ..Default::default()
            },
            add(id, "Original"),
            add(id, "Duplicate"),
        ]
    ));
    let player = roster.get(&id).unwrap();
    assert_eq!(player.profile.name.as_ref(), "Original");
    assert_eq!(player.latency, 42);
    assert_eq!(player.mode, GameMode::Survival);
    assert!(player.show_hat);
    assert!(!player.listed);
    assert_eq!(roster.unknown_updates, 0);
}

#[test]
fn nullable_updates_removal_readdition_and_retired_sessions_are_distinct() {
    let mut roster = Roster::new(7);
    let id = [1; 16];
    roster.update(7, vec![add(id, "Original")]);
    let profile = Arc::clone(&roster.get(&id).unwrap().profile);
    roster.update(
        7,
        vec![Update {
            uuid: id,
            listed: Some(true),
            display_name: Some(Some(NbtValue::String("Display".into()))),
            show_hat: Some(false),
            mode: Some(GameMode::Creative),
            ..Default::default()
        }],
    );
    assert!(!roster.remove(6, &[id]));
    assert!(!roster.update(
        6,
        vec![Update {
            uuid: id,
            listed: Some(false),
            ..Default::default()
        }]
    ));
    let player = roster.get(&id).unwrap();
    assert!(player.listed && player.display_name.is_some());
    assert_eq!(player.mode, GameMode::Creative);
    assert!(!player.show_hat);
    assert!(Arc::ptr_eq(&profile, &player.profile));
    roster.update(
        7,
        vec![Update {
            uuid: id,
            display_name: Some(None),
            ..Default::default()
        }],
    );
    assert!(roster.get(&id).unwrap().display_name.is_none());
    roster.remove(7, &[id]);
    roster.update(
        7,
        vec![Update {
            uuid: id,
            listed: Some(true),
            ..Default::default()
        }],
    );
    assert!(roster.get(&id).is_none());
    assert_eq!(roster.unknown_updates, 1);
    roster.update(7, vec![add(id, "New")]);
    assert!(!Arc::ptr_eq(&profile, &roster.get(&id).unwrap().profile));
    assert_eq!(profile.name.as_ref(), "Original");
    assert_eq!(roster.get(&id).unwrap().profile.name.as_ref(), "New");
}
