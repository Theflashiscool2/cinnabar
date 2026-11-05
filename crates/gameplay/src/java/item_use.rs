//! Native use planning/lifetimes. The legacy instant-only admission below remains
//! for entity fallback; ordinary block/air use uses plan and active.
use inventory::java::{Slot, cooldown::Cooldowns};
pub mod active;
pub mod block;
pub mod plan;

#[derive(Clone, Copy)]
pub struct Defaults<'a> {
    pub duration: u32,
    pub group: &'a str,
}
/// Resolves authoritative patches before admitting a native instant-use request.
/// False is a normal cooldown gate; it must not allocate an action sequence.
pub fn instant(
    held: Option<&Slot>,
    defaults: Option<Defaults<'_>>,
    cooldowns: &Cooldowns,
) -> Result<bool, &'static str> {
    let slot = held.ok_or("Native held item is not known")?;
    let stack = slot
        .as_ref()
        .ok_or("Native offhand air-use fallback is not implemented")?;
    let defaults = defaults.ok_or("Unknown native item-use defaults")?;
    if defaults.duration != 0
        || [
            "minecraft:consumable",
            "minecraft:blocks_attacks",
            "minecraft:kinetic_weapon",
        ]
        .iter()
        .any(|key| stack.added.contains_key(*key))
    {
        return Err("Native component-defined or active item use/release is not implemented");
    }
    let key = "minecraft:use_cooldown";
    let group = if let Some(value) = stack.added.get(key) {
        let object = value
            .as_object()
            .ok_or("Invalid native item cooldown component")?;
        match object.get("group") {
            Some(serde_json::Value::String(group)) => group.as_str(),
            Some(serde_json::Value::Null) => stack.item.as_str(),
            _ => return Err("Invalid native item cooldown group"),
        }
    } else if stack.removed.contains(key) {
        stack.item.as_str()
    } else {
        defaults.group
    };
    Ok(!cooldowns.active(group))
}

#[cfg(test)]
mod tests {
    use super::*;
    use inventory::java::Stack;
    use serde_json::json;
    #[test]
    fn grouped_cooldown_and_authoritative_patches_gate_instant_use() {
        let mut held = Some(Stack {
            item: "fixture:item".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        });
        let d = Some(Defaults {
            duration: 0,
            group: "fixture:default",
        });
        let mut c = Cooldowns::default();
        c.set("fixture:default".into(), 20).unwrap();
        assert_eq!(instant(Some(&held), d, &c), Ok(false));
        held.as_mut()
            .unwrap()
            .removed
            .insert("minecraft:use_cooldown".into());
        assert_eq!(instant(Some(&held), d, &c), Ok(true));
        let s = held.as_mut().unwrap();
        s.added.insert(
            "minecraft:use_cooldown".into(),
            json!({"seconds":1,"group":"fixture:shared"}),
        );
        c.set("fixture:shared".into(), 4).unwrap();
        assert_eq!(instant(Some(&held), d, &c), Ok(false));
        c.advance(4);
        assert_eq!(instant(Some(&held), d, &c), Ok(true));
        held.as_mut()
            .unwrap()
            .added
            .insert("minecraft:consumable".into(), json!({"seconds":1.6}));
        assert!(instant(Some(&held), d, &c).is_err());
    }
    #[test]
    fn active_unknown_and_empty_hands_never_fall_through_as_instant_use() {
        let held = Some(Stack {
            item: "fixture:item".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        });
        let c = Cooldowns::default();
        assert!(
            instant(
                Some(&held),
                Some(Defaults {
                    duration: 32,
                    group: "fixture:item"
                }),
                &c
            )
            .is_err()
        );
        assert!(instant(None, None, &c).is_err());
        assert!(instant(Some(&None), None, &c).is_err());
        assert!(instant(Some(&held), None, &c).is_err());
    }
}
