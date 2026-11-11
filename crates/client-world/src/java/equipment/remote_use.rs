//! Native remote use snapshots live on the admitted entity; equipment remains appearance data.
use super::*;
use inventory::java::Stack;
use std::sync::Arc;
use world::java::entities::{
    animation::PlayerUse,
    visual::{RemoteUse, Visual},
};

fn component<'a>(stack: &'a Stack, name: &str) -> Option<&'a serde_json::Value> {
    if stack.removed.contains(name) {
        None
    } else {
        stack
            .added
            .get(name)
            .or_else(|| assets::registry::java::items::defaults::component(&stack.item, name))
    }
}
fn duration(stack: &Stack) -> Option<i32> {
    let index = java_protocol::target::ITEM_TYPES
        .iter()
        .position(|name| *name == stack.item)?;
    let native = i32::try_from(java_protocol::target::ITEM_USE_DEFAULTS[index].duration).ok()?;
    match stack.item.as_str() {
        // BowItem overrides duration and animation independently of component patches.
        "minecraft:bow" => Some(native),
        "minecraft:shield" if component(stack, "minecraft:consumable").is_none() => Some(
            if component(stack, "minecraft:blocks_attacks").is_some()
                || component(stack, "minecraft:kinetic_weapon").is_some()
            {
                native
            } else {
                0
            },
        ),
        _ => None,
    }
}
fn held<'a>(gear: Option<&'a Equipment>, visual: &Visual) -> Option<&'a Stack> {
    gear?.slots[usize::from(visual.use_flags & 2 != 0)]
        .as_ref()
        .filter(|stack| stack.count > 0)
}
/// Mirrors LivingEntity.onSyncedDataUpdated: a flag receipt starts only an empty use snapshot.
pub(in crate::java) fn observe(state: &State, identity: Identity, visual: &mut Visual) {
    if visual.use_flags & 1 == 0 {
        visual.remote_use = None;
        return;
    }
    let gear = state.get(identity);
    let stack = held(gear, visual);
    let token = gear.zip(stack).map(|(gear, _)| {
        let slot = u8::from(visual.use_flags & 2 != 0);
        (slot, gear.slot_revisions[usize::from(slot)])
    });
    if let Some(using) = &mut visual.remote_use {
        using.observed_item = stack.map(|stack| Arc::from(stack.item.as_str()));
        using.observed_token = token;
    } else if let Some(stack) = stack {
        let item = Arc::from(stack.item.as_str());
        visual.remote_use = Some(RemoteUse {
            observed_item: Some(Arc::clone(&item)),
            observed_token: token,
            item_token: token.expect("nonempty native use stack has a slot receipt"),
            item,
            remaining: duration(stack),
        });
    }
}
/// Current hand data selects pose, while the captured native remaining duration gates active use.
pub fn pose(visual: &Visual, gear: Option<&Equipment>) -> Option<PlayerUse> {
    visual
        .remote_use
        .as_ref()?
        .remaining
        .filter(|remaining| *remaining > 0)?;
    if visual.use_flags & 1 == 0 {
        return None;
    }
    let stack = held(gear, visual)?;
    let left = (visual.use_flags & 2 != 0) != visual.left_handed;
    match stack.item.as_str() {
        "minecraft:bow" => Some(PlayerUse::Bow { left }),
        "minecraft:shield"
            if component(stack, "minecraft:consumable").is_none()
                && component(stack, "minecraft:blocks_attacks").is_some() =>
        {
            Some(PlayerUse::Block { left })
        }
        _ => None,
    }
}

/// Native using_item property compares the captured ItemStack reference, not merely item type.
pub fn model_using(visual: &Visual, gear: &Equipment, slot: u8) -> bool {
    let Some(using) = &visual.remote_use else {
        return false;
    };
    let used_slot = u8::from(visual.use_flags & 2 != 0);
    visual.use_flags & 1 != 0
        && slot == used_slot
        && gear
            .slot_revisions
            .get(usize::from(slot))
            .is_some_and(|revision| using.item_token == (slot, *revision))
        && held(Some(gear), visual).is_some_and(|stack| stack.item.as_str() == using.item.as_ref())
}
/// Only native bow/default shield owner-use dispatch is currently known; other specialized
/// contexts retain the existing ownerless path instead of inventing duration observations.
pub fn model_owner(
    visual: &Visual,
    gear: &Equipment,
    slot: u8,
    entity_type: Arc<str>,
) -> Option<assets::registry::java::items::OwnerObservations> {
    let stack = gear.slots.get(usize::from(slot))?.as_ref()?;
    if !matches!(stack.item.as_str(), "minecraft:bow" | "minecraft:shield") {
        return None;
    }
    let using_this_item = model_using(visual, gear, slot);
    Some(assets::registry::java::items::OwnerObservations {
        entity_type,
        main_arm_left: visual.left_handed,
        using_this_item,
        use_duration: duration(stack),
        use_item_remaining_ticks: if using_this_item {
            visual.remote_use.as_ref().and_then(|using| using.remaining)
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stack(item: &str) -> Stack {
        Stack {
            item: item.into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        }
    }
    #[test]
    fn native_used_hand_and_main_arm_select_bow_side_and_shield_removal_disables_block() {
        let id = Identity {
            session: 7,
            epoch: 1,
            incarnation: 1,
        };
        for main_left in [false, true] {
            for offhand in [false, true] {
                let mut state = State::default();
                state
                    .apply(
                        id,
                        [
                            (0, Some(stack("minecraft:bow"))),
                            (1, Some(stack("minecraft:bow"))),
                        ]
                        .into(),
                    )
                    .unwrap();
                let mut visual = Visual {
                    left_handed: main_left,
                    ..Default::default()
                };
                visual.observe_use(1 | if offhand { 2 } else { 0 }).unwrap();
                observe(&state, id, &mut visual);
                assert_eq!(
                    pose(&visual, state.get(id)),
                    Some(PlayerUse::Bow {
                        left: offhand != main_left
                    })
                );
            }
        }
        let mut state = State::default();
        let mut shield = stack("minecraft:shield");
        shield.removed.insert("minecraft:blocks_attacks".into());
        state.apply(id, [(0, Some(shield))].into()).unwrap();
        let mut visual = Visual::default();
        visual.observe_use(1).unwrap();
        observe(&state, id, &mut visual);
        assert_eq!(visual.remote_use.as_ref().unwrap().remaining, Some(0));
        assert_eq!(pose(&visual, state.get(id)), None);
        let mut shield = stack("minecraft:shield");
        shield.added.insert(
            "minecraft:consumable".into(),
            serde_json::json!({"seconds":0.5}),
        );
        state.apply(id, [(0, Some(shield))].into()).unwrap();
        visual.observe_use(0).unwrap();
        visual.observe_use(1).unwrap();
        observe(&state, id, &mut visual);
        assert_eq!(
            visual.remote_use.as_ref().unwrap().remaining,
            None,
            "specialized override duration remains an explicit gap"
        );
        assert_eq!(pose(&visual, state.get(id)), None);
    }
}
