//! Native AdventureModePredicate evaluation and its component-owned last-state cache.
use super::*;
use assets::registry::java::{
    BlockState,
    adventure::{Permission, Table},
    items::defaults,
};
#[derive(Debug, Default)]
pub(super) struct Cache {
    entries: BTreeMap<(usize, Permission), (u32, bool)>,
}
/// Immutable facts from the one admitted native world; packet NBT is not canonical BE data.
pub struct Target<'a> {
    pub state: u32,
    pub block: &'a BlockState,
    pub tags: &'a BTreeMap<String, Vec<i32>>,
    pub tag_revision: u64,
}
fn component(stack: &Stack, permission: Permission) -> Option<&Value> {
    let key = permission.component();
    if stack.removed.contains(key) {
        None
    } else {
        stack
            .added
            .get(key)
            .or_else(|| defaults::component(&stack.item, key))
    }
}
fn permissions(stack: &Stack) -> impl Iterator<Item = (Permission, &Value)> {
    [Permission::Break, Permission::PlaceOn]
        .into_iter()
        .filter_map(|permission| component(stack, permission).map(|value| (permission, value)))
}
fn tagged(value: &Value) -> bool {
    value.as_array().is_some_and(|predicates| {
        predicates.iter().any(|predicate| {
            predicate
                .get("blocks")
                .is_some_and(|blocks| blocks.get("tag").is_some())
        })
    })
}
impl State {
    /// Tests the current hand's decoded component, preserving native same-state cache hits.
    /// Authoritative stack replacements invalidate the component cache; count-only projections do not.
    pub fn test_adventure(
        &self,
        hand: BlockUseHand,
        permission: Permission,
        target: Target<'_>,
    ) -> Result<bool, &'static str> {
        if self.error.is_some() {
            return Err("Native Adventure inventory is not known");
        }
        let slot = self
            .hand_slot(hand)
            .ok_or("Native Adventure hand slot is not known")?;
        let held = self
            .player_slot_view(slot)
            .ok_or("Native Adventure held item is not known")?;
        let Some(stack) = held.as_ref().filter(|stack| stack.count > 0) else {
            return Ok(false);
        };
        let Some(value) = component(stack, permission) else {
            return Ok(false);
        };
        if target.tag_revision != 0 && self.adventure_copied.contains(&slot) && tagged(value) {
            return Err(
                "Native Adventure predicate cache identity after local copy and tag rebinding is unavailable",
            );
        }
        let mut cache = self
            .adventure_cache
            .lock()
            .expect("Adventure cache poisoned");
        if let Some((state, result)) = cache.entries.get(&(slot, permission))
            && *state == target.state
        {
            return Ok(*result);
        }
        let result = evaluate(value, &target)?;
        cache
            .entries
            .insert((slot, permission), (target.state, result));
        Ok(result)
    }
    pub(super) fn retire_adventure_pickup_view(&mut self) {
        let entries = &mut self
            .adventure_cache
            .get_mut()
            .expect("Adventure cache poisoned")
            .entries;
        for slot in self.pickup_player_view.keys() {
            entries.retain(|(index, _), _| index != slot);
        }
    }
    pub(super) fn replace_adventure_slot(&mut self, slot: usize) {
        self.adventure_cache
            .get_mut()
            .expect("Adventure cache poisoned")
            .entries
            .retain(|(index, _), _| *index != slot);
        self.adventure_copied.remove(&slot);
    }
    /// Pickup copies/merges share native predicate objects. Retire derived slot caches and
    /// retain explicit ambiguity for later tag rebinding instead of inventing object identity.
    pub(super) fn adventure_pickup(&mut self, plan: &pickup::Plan) {
        let mut affected = Vec::new();
        for (index, after) in &plan.changed {
            for stack in self
                .slot(plan.identity.container, *index)
                .into_iter()
                .chain(std::iter::once(after))
                .filter_map(Option::as_ref)
            {
                affected.extend(
                    permissions(stack).map(|(permission, value)| (permission, value.clone())),
                );
            }
        }
        for stack in self
            .carried_view(plan.identity.container)
            .into_iter()
            .chain(std::iter::once(&plan.carried))
            .filter_map(Option::as_ref)
        {
            affected
                .extend(permissions(stack).map(|(permission, value)| (permission, value.clone())));
        }
        if affected.is_empty() {
            return;
        }
        for index in 0..self.storage.len() {
            if self
                .player_slot_view(index)
                .and_then(Option::as_ref)
                .is_some_and(|stack| {
                    permissions(stack).any(|(permission, value)| {
                        affected
                            .iter()
                            .any(|(kind, candidate)| *kind == permission && candidate == value)
                    })
                })
            {
                self.adventure_copied.insert(index);
                self.adventure_cache
                    .get_mut()
                    .expect("Adventure cache poisoned")
                    .entries
                    .retain(|(slot, _), _| *slot != index);
            }
        }
        for (index, after) in &plan.changed {
            if after
                .as_ref()
                .is_some_and(|stack| permissions(stack).next().is_some())
                && let Some(Binding::Player(slot)) = self
                    .menu(plan.identity.container)
                    .and_then(|menu| menu.slots.get(*index))
            {
                let slot = *slot;
                self.adventure_copied.insert(slot);
                self.adventure_cache
                    .get_mut()
                    .expect("Adventure cache poisoned")
                    .entries
                    .retain(|(index, _), _| *index != slot);
            }
        }
    }
}
fn evaluate(value: &Value, target: &Target<'_>) -> Result<bool, &'static str> {
    let facts = assets::registry::java::adventure::builtin()
        .map_err(|_| "Invalid pinned Adventure property facts")?;
    let (id, block) = facts
        .block(&target.block.name)
        .ok_or("Unknown native Adventure block identity")?;
    for predicate in value
        .as_array()
        .ok_or("Invalid native Adventure predicate list")?
    {
        if let Some(blocks) = predicate.get("blocks").filter(|value| !value.is_null()) {
            let matches = if let Some(tag) = blocks.get("tag").and_then(Value::as_str) {
                target
                    .tags
                    .get(tag)
                    .is_some_and(|ids| ids.contains(&(id as i32)))
            } else if let Some(ids) = blocks.get("ids").and_then(Value::as_array) {
                ids.iter()
                    .any(|candidate| candidate.as_u64() == Some(u64::from(id)))
            } else {
                return Err("Invalid native Adventure block holder set");
            };
            if !matches {
                continue;
            }
        }
        if !properties_match(
            facts,
            block,
            predicate.get("properties"),
            &target.block.properties,
        )? {
            continue;
        }
        if predicate.get("nbt").is_some_and(|nbt| !nbt.is_null()) {
            if block.block_entity {
                return Err(
                    "Native Adventure NBT predicate requires canonical BlockEntity.saveWithFullMetadata",
                );
            }
            continue; // Native matchesBlockEntityData is false without a block entity, even {}.
        }
        // BlockPredicate.matches(BlockInWorld) does not test DataComponentMatchers.
        return Ok(true);
    }
    Ok(false)
}
fn properties_match(
    facts: &Table,
    block: &assets::registry::java::adventure::Block,
    properties: Option<&Value>,
    current: &BTreeMap<String, String>,
) -> Result<bool, &'static str> {
    let Some(properties) = properties.filter(|value| !value.is_null()) else {
        return Ok(true);
    };
    for matcher in properties
        .as_array()
        .ok_or("Invalid native Adventure property matchers")?
    {
        let name = matcher
            .get("name")
            .and_then(Value::as_str)
            .ok_or("Invalid native Adventure property name")?;
        let Some(property) = block.properties.get(name) else {
            return Ok(false);
        };
        let Some(actual) = current
            .get(name)
            .and_then(|value| facts.property_index(property, value))
        else {
            return Ok(false);
        };
        if let Some(exact) = matcher.get("value") {
            let exact = exact
                .as_str()
                .ok_or("Invalid native Adventure property value")?;
            if facts.property_index(property, exact) != Some(actual) {
                return Ok(false);
            }
        } else {
            for (bound, minimum) in [("min", true), ("max", false)] {
                if let Some(value) = matcher.get(bound).filter(|value| !value.is_null()) {
                    let value = value
                        .as_str()
                        .ok_or("Invalid native Adventure property range")?;
                    let Some(index) = facts.property_index(property, value) else {
                        return Ok(false);
                    };
                    if minimum && actual < index || !minimum && actual > index {
                        return Ok(false);
                    }
                }
            }
        }
    }
    Ok(true)
}
#[cfg(test)]
mod tests;
