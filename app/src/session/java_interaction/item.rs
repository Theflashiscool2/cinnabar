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
