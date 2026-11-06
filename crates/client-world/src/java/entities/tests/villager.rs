use super::*;
use java_protocol::target;

fn villager(lifetime: u64) -> wire::Packet {
    let wire::Packet::Spawn(mut value) = spawn() else {
        unreachable!()
    };
    value.kind = target::ENTITY_VILLAGER;
    value.lifetime = lifetime;
    wire::Packet::Spawn(value)
}
fn metadata(name: &str, value: wire::metadata::Value) -> wire::Packet {
    let spec = target::METADATA_VILLAGER
        .iter()
        .find(|s| s.name == name)
        .unwrap();
    wire::Packet::Metadata(wire::metadata::Packet {
        id: 99,
        entries: vec![wire::metadata::Entry {
            index: spec.index,
            serializer: spec.serializer,
            value,
        }],
        unsupported: None,
    })
}
#[test]
fn villager_native_defaults_variants_baby_timer_and_replacement() {
    let mut s = state();
    assert!(s.admit(event(1, villager(1))));
    let e = s.entities.get(99).unwrap();
    assert!(e.presentation.is_some());
    let a = e.visual.villager.as_ref().unwrap();
    assert_eq!(a.variant.as_ref(), "minecraft:plains");
    assert_eq!(a.profession.as_ref(), "minecraft:none");
    assert_eq!(a.level, 1);
    let snow = target::VILLAGER_TYPES
        .iter()
        .position(|s| *s == "minecraft:snow")
        .unwrap() as i32;
    let farmer = target::VILLAGER_PROFESSIONS
        .iter()
        .position(|s| *s == "minecraft:farmer")
        .unwrap() as i32;
    assert!(s.admit(event(
        1,
        metadata(
            "DATA_VILLAGER_DATA",
            wire::metadata::Value::Villager([snow, farmer, 999])
        )
    )));
    let a = s
        .entities
        .get(99)
        .unwrap()
        .visual
        .villager
        .as_ref()
        .unwrap();
    assert_eq!(a.variant.as_ref(), "minecraft:snow");
    assert_eq!(a.profession.as_ref(), "minecraft:farmer");
    assert_eq!(a.level, 999);
    assert!(s.admit(event(
        1,
        metadata("DATA_BABY_ID", wire::metadata::Value::Bool(true))
    )));
    assert!(s.entities.get(99).unwrap().visual.baby);
    assert!(s.admit(event(
        1,
        metadata("DATA_UNHAPPY_COUNTER", wire::metadata::Value::Int(2))
    )));
    s.entities.tick_presentation();
    assert_eq!(
        s.entities
            .get(99)
            .unwrap()
            .visual
            .villager
            .as_ref()
            .unwrap()
            .unhappy_ticks,
        1
    );
    s.entities.tick_presentation();
    s.entities.tick_presentation();
    assert_eq!(
        s.entities
            .get(99)
            .unwrap()
            .visual
            .villager
            .as_ref()
            .unwrap()
            .unhappy_ticks,
        0
    );
    assert!(s.admit(event(1, wire::Packet::Remove(vec![99]))));
    assert!(s.admit(event(1, villager(2))));
    assert_eq!(
        s.entities
            .get(99)
            .unwrap()
            .visual
            .villager
            .as_ref()
            .unwrap()
            .profession
            .as_ref(),
        "minecraft:none"
    );
    assert!(!s.entities.get(99).unwrap().visual.baby);
    assert!(!s.admit(event(
        2,
        metadata("DATA_UNHAPPY_COUNTER", wire::metadata::Value::Int(20))
    )));
    s.begin(8);
    assert!(s.entities.get(99).is_none());
}
#[test]
fn villager_invalid_builtin_identity_does_not_partially_publish() {
    let mut s = state();
    assert!(s.admit(event(1, villager(1))));
    let old = s.entities.get(99).unwrap().visual.villager.clone();
    assert!(!s.admit(event(
        1,
        metadata(
            "DATA_VILLAGER_DATA",
            wire::metadata::Value::Villager([0, i32::MAX, 2])
        )
    )));
    assert_eq!(s.entities.get(99).unwrap().visual.villager, old);
}
