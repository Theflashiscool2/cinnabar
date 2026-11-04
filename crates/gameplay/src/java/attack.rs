//! Main-hand attack dispatch uses native item defaults plus the authoritative component patch.
use inventory::java::{Slot, Stack};
#[derive(Clone, Copy, Debug, Default)]
pub struct Defaults {
    pub range: bool,
    pub piercing: bool,
    pub minimum_charge: f64,
}

/// Ordinary attacks share the native Attack/Punch path. Charged, piercing and custom-range
/// attacks require their own targeting/timing, and must not silently use an unarmed ray.
pub fn ordinary(held: Option<&Slot>, defaults: Option<Defaults>) -> Result<(), &'static str> {
    let held = held.ok_or("Native held item is not known")?;
    let Some(stack) = held else { return Ok(()) };
    let defaults = defaults.ok_or("Unknown native attack item defaults")?;
    if present(stack, "minecraft:attack_range", defaults.range)
        || present(stack, "minecraft:piercing_weapon", defaults.piercing)
    {
        return Err("Native special-range or piercing item attacks are not implemented");
    }
    let key = "minecraft:minimum_attack_charge";
    let charge = if let Some(value) = stack.added.get(key) {
        value
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.)
            .ok_or("Invalid native minimum attack charge")?
    } else if stack.removed.contains(key) {
        0.
    } else {
        defaults.minimum_charge
    };
    if charge > 0. {
        return Err("Native charged item attacks are not implemented");
    }
    Ok(())
}

fn present(stack: &Stack, key: &str, default: bool) -> bool {
    stack.added.contains_key(key) || (default && !stack.removed.contains(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(name: &str) -> Slot {
        Some(Stack {
            item: name.into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        })
    }
    #[test]
    fn native_defaults_admit_ordinary_swords_axes_and_items_but_not_spears() {
        assert!(ordinary(None, None).is_err());
        assert!(ordinary(Some(&None), None).is_ok());
        for name in [
            "minecraft:iron_sword",
            "minecraft:diamond_axe",
            "minecraft:gold_ingot",
        ] {
            assert!(
                ordinary(Some(&item(name)), Some(Defaults::default())).is_ok(),
                "{name}"
            );
        }
        assert!(ordinary(Some(&item("fixture:unknown")), None).is_err());
    }
    #[test]
    fn authoritative_component_overrides_and_removals_resolve_before_dispatch() {
        let mut sword = item("minecraft:iron_sword");
        for (key, value) in [
            ("minecraft:attack_range", json!({"max": 8})),
            ("minecraft:piercing_weapon", json!({})),
            ("minecraft:minimum_attack_charge", json!(0.5)),
            ("minecraft:minimum_attack_charge", json!("malformed")),
        ] {
            sword.as_mut().unwrap().added.insert(key.into(), value);
            assert!(ordinary(Some(&sword), Some(Defaults::default())).is_err());
            sword.as_mut().unwrap().added.clear();
        }
        let mut spear = item("minecraft:iron_spear");
        let defaults = Some(Defaults {
            range: true,
            piercing: true,
            minimum_charge: 1.,
        });
        assert!(ordinary(Some(&spear), defaults).is_err());
        let s = spear.as_mut().unwrap();
        for key in [
            "minecraft:attack_range",
            "minecraft:piercing_weapon",
            "minecraft:minimum_attack_charge",
        ] {
            s.removed.insert(key.into());
        }
        assert!(ordinary(Some(&spear), defaults).is_ok());
        spear
            .as_mut()
            .unwrap()
            .added
            .insert("minecraft:minimum_attack_charge".into(), json!(1));
        assert!(ordinary(Some(&spear), defaults).is_err());
    }
}
