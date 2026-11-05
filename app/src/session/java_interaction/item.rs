//! Maps pinned defaults at the application boundary; domain policy resolves server patches.
pub(super) fn instant(state: &client_world::java::WorldState) -> Result<bool, &'static str> {
    let held = state.inventory.held();
    let defaults = held.and_then(|s| s.as_ref()).and_then(|s| {
        let id = java_protocol::target::ITEM_TYPES
            .iter()
            .position(|name| *name == s.item)?;
        let d = java_protocol::target::ITEM_USE_DEFAULTS[id];
        Some(gameplay::java::item_use::Defaults {
            duration: d.duration,
            group: d.group,
        })
    });
    gameplay::java::item_use::instant(held, defaults, &state.inventory.cooldowns)
}

pub(super) fn plan(
    state: &client_world::java::WorldState,
) -> Result<gameplay::java::item_use::plan::Plan, &'static str> {
    plan_for(state, false)
}
pub(super) fn resume(
    state: &client_world::java::WorldState,
) -> Result<gameplay::java::item_use::plan::Plan, &'static str> {
    plan_for(state, true)
}
fn plan_for(
    state: &client_world::java::WorldState,
    authoritative: bool,
) -> Result<gameplay::java::item_use::plan::Plan, &'static str> {
    let held = state.inventory.held();
    let defaults = held.and_then(|s| s.as_ref()).and_then(|s| {
        let id = java_protocol::target::ITEM_TYPES
            .iter()
            .position(|name| *name == s.item)?;
        let d = java_protocol::target::ITEM_USE_DEFAULTS[id];
        Some(gameplay::java::item_use::Defaults {
            duration: d.duration,
            group: d.group,
        })
    });
    let abilities = state.player_dynamics.abilities.as_ref();
    let arrows = state
        .bootstrap
        .as_ref()
        .and_then(|b| b.configuration.tags.get("minecraft:item"))
        .and_then(|t| t.get("minecraft:arrows"));
    let projectile = (0..inventory::java::PLAYER_MENU_SIZE).any(|slot| {
        state
            .inventory
            .slot(0, slot)
            .and_then(Option::as_ref)
            .is_some_and(|s| {
                java_protocol::target::ITEM_TYPES
                    .iter()
                    .position(|n| *n == s.item)
                    .is_some_and(|id| arrows.is_some_and(|ids| ids.contains(&(id as i32))))
            })
    });
    let empty_cooldowns = inventory::java::cooldown::Cooldowns::default();
    gameplay::java::item_use::plan::prepare(
        held,
        defaults,
        if authoritative {
            &empty_cooldowns
        } else {
            &state.inventory.cooldowns
        },
        gameplay::java::item_use::plan::Conditions {
            food: state.vitals.as_ref().map(|v| v.food),
            invulnerable: authoritative || abilities.is_some_and(|a| a.invulnerable),
            creative: authoritative || abilities.is_some_and(|a| a.creative),
            projectile,
        },
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn pinned_native_use_defaults_distinguish_instant_and_active_items() {
        use java_protocol::target;
        assert_eq!(target::ITEM_TYPES.len(), target::ITEM_USE_DEFAULTS.len());
        let duration = |name| {
            target::ITEM_USE_DEFAULTS[target::ITEM_TYPES.iter().position(|n| *n == name).unwrap()]
                .duration
        };
        assert_eq!(duration("minecraft:compass"), 0);
        assert_eq!(duration("minecraft:snowball"), 0);
        assert_eq!(duration("minecraft:bread"), 32);
        assert_eq!(duration("minecraft:bow"), 72000);
    }
}
