//! Default components and tint facts exported from the exact pinned native runtime.
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(serde::Deserialize)]
struct Facts {
    item_render_defaults: BTreeMap<String, BTreeMap<String, Value>>,
    potion_colors: BTreeMap<String, Option<i32>>,
    effect_colors: BTreeMap<String, i32>,
    potion_ids: Vec<String>,
    effect_ids: Vec<String>,
    potion_effects: BTreeMap<String, Vec<Value>>,
    #[serde(skip)]
    item_ids: Vec<String>,
}
fn facts() -> &'static Facts {
    static FACTS: OnceLock<Facts> = OnceLock::new();
    FACTS.get_or_init(|| {
        use sha2::{Digest, Sha256};
        let bytes = include_bytes!("../../../../../java-protocol/data/entity-facts.json");
        let target: Value = serde_json::from_slice(include_bytes!(
            "../../../../../java-protocol/data/target.json"
        ))
        .expect("Java target metadata");
        assert_eq!(
            bytes.len() as u64,
            target["entity_facts_report"]["bytes"].as_u64().unwrap()
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            target["entity_facts_report"]["sha256"].as_str().unwrap()
        );
        let mut facts: Facts =
            serde_json::from_slice(bytes).expect("generated native item components");
        let names: Vec<String> = serde_json::from_slice(include_bytes!(
            "../../../../../java-protocol/data/item-types.json"
        ))
        .expect("native item names");
        assert_eq!(names.len(), facts.item_render_defaults.len());
        assert!(
            names
                .iter()
                .all(|n| facts.item_render_defaults.contains_key(n))
        );
        facts.item_ids = names;
        facts
    })
}
pub(super) fn components(item: &str) -> Option<&'static BTreeMap<String, Value>> {
    facts().item_render_defaults.get(item)
}
/// Resolves a target default without borrowing a different edition's registry or numeric identity.
pub fn component(item: &str, name: &str) -> Option<&'static Value> {
    components(item)?.get(name)
}
pub(super) fn item_name(value: &Value) -> Option<&str> {
    value.as_str().or_else(|| {
        value
            .as_u64()
            .and_then(|i| facts().item_ids.get(i as usize))
            .map(String::as_str)
    })
}
fn name<'a>(value: &'a Value, ids: &'static [String]) -> Option<&'a str> {
    value.as_str().or_else(|| {
        value
            .as_u64()
            .and_then(|i| ids.get(i as usize))
            .map(String::as_str)
    })
}
/// Resolves the target's built-in potion wire identity; custom effects do not
/// change the base potion. Callers must separately apply PotionContents.is rules.
pub fn potion_name(contents: &Value) -> Option<&str> {
    contents
        .get("potion")
        .and_then(|p| name(p, &facts().potion_ids))
}
/// PotionItem's native mud conversion uses WATER without any custom effects.
pub fn is_plain_water(contents: &Value) -> Result<bool, &'static str> {
    let base = potion_name(contents);
    if contents.get("potion").is_some_and(|p| !p.is_null())
        && base.is_none_or(|p| !facts().potion_ids.iter().any(|n| n == p))
    {
        return Err("Unknown native potion identity");
    }
    let custom = contents
        .get("effects")
        .or_else(|| contents.get("custom_effects"));
    let has_custom = custom.map_or(Ok(false), |v| {
        v.as_array()
            .map(|a| !a.is_empty())
            .ok_or("Invalid native potion effects")
    })?;
    Ok(base == Some("minecraft:water") && !has_custom)
}
pub(super) fn potion_color(contents: &Value) -> Option<i32> {
    if let Some(color) = contents
        .get("custom_color")
        .and_then(Value::as_i64)
        .or_else(|| contents.get("color").and_then(Value::as_i64))
    {
        return Some(color as i32);
    }
    let base = contents
        .get("potion")
        .and_then(|p| name(p, &facts().potion_ids));
    let custom = contents
        .get("custom_effects")
        .or_else(|| contents.get("effects"))
        .and_then(Value::as_array);
    if custom.is_none_or(|c| c.is_empty()) {
        return base
            .and_then(|p| facts().potion_colors.get(p))
            .copied()
            .flatten();
    }
    let effects = base
        .and_then(|p| facts().potion_effects.get(p))
        .into_iter()
        .flatten()
        .chain(custom.into_iter().flatten());
    let mut total = 0i32;
    let mut rgb = [0i32; 3];
    for effect in effects {
        let details = effect.get("details").unwrap_or(effect);
        if !details
            .get("show_particles")
            .or_else(|| details.get("particles"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        let Some(color) = effect
            .get("id")
            .and_then(|id| name(id, &facts().effect_ids))
            .and_then(|id| facts().effect_colors.get(id))
        else {
            continue;
        };
        let weight = (details
            .get("amplifier")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32)
            .wrapping_add(1);
        total = total.wrapping_add(weight);
        for (i, c) in rgb.iter_mut().enumerate() {
            *c = c.wrapping_add(weight.wrapping_mul((color >> (16 - 8 * i)) & 255));
        }
    }
    if total == 0 {
        return None;
    }
    let rgb = rgb.map(|c| c.wrapping_div(total) as u32);
    Some((0xff000000 | rgb[0] << 16 | rgb[1] << 8 | rgb[2]) as i32)
}
pub(super) fn validate_potion(contents: &Value) -> bool {
    if let Some(p) = contents.get("potion").filter(|p| !p.is_null())
        && name(p, &facts().potion_ids)
            .is_none_or(|p| !facts().potion_ids.iter().any(|name| name == p))
    {
        return false;
    }
    contents
        .get("custom_effects")
        .or_else(|| contents.get("effects"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .all(|e| {
            e.get("id")
                .and_then(|id| name(id, &facts().effect_ids))
                .is_some_and(|id| facts().effect_colors.contains_key(id))
        })
}
