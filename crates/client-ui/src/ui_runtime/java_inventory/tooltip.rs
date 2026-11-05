//! Native server menu hover text, reusing the shared bounded component formatter.
use super::{component, component_title_with, stack_name};
use crate::ui_runtime::presentation::nametags::java::text;
use assets::registry::java::language::Language;
use inventory::java::Stack;
use serde_json::Value;
use world::{NbtCompound, NbtValue};

const UNSUPPORTED: &str = "§r§8[Unsupported Java tooltip text]";

/// Rebuilt only with a new inventory snapshot or item-name resource, never per painted cell.
#[derive(Clone, Debug, Default)]
pub(super) struct Cache {
    pub(super) player: Vec<String>,
    pub(super) opened: Vec<String>,
    pub(super) player_carried: String,
    pub(super) opened_carried: String,
}
impl Cache {
    pub(super) fn build(p: &super::Projection, names: Option<&Language>) -> Self {
        let text = |s: &Option<inventory::java::Slot>| {
            s.as_ref()
                .and_then(Option::as_ref)
                .map_or_else(String::new, |s| hover(s, names))
        };
        Self {
            player: p.player.iter().map(text).collect(),
            opened: p
                .opened
                .as_ref()
                .map_or_else(Vec::new, |m| m.slots.iter().map(text).collect()),
            player_carried: text(&p.player_carried),
            opened_carried: p
                .opened
                .as_ref()
                .map_or_else(String::new, |m| text(&m.carried)),
        }
    }
}

pub(super) fn hover(stack: &Stack, names: Option<&Language>) -> String {
    let display = component(stack, "minecraft:tooltip_display");
    if display
        .and_then(|v| v.get("hide_tooltip"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return String::new();
    }
    let mut budget = Budget::default();
    let custom = component(stack, "minecraft:custom_name");
    let name = custom.or_else(|| stack.added.get("minecraft:item_name"));
    let translate = |key: &str| names.and_then(|n| n.get(key)).map(str::to_owned);
    let mut unsupported = false;
    let mut result = match name {
        Some(value) => formatted(value, custom.is_some(), None, &mut budget).unwrap_or_else(|_| {
            unsupported = true;
            component_title_with(value, &translate)
        }),
        None => stack_name(stack, names),
    };
    let hidden = display
        .and_then(|v| v.get("hidden_components"))
        .and_then(Value::as_array)
        .is_some_and(|v| v.iter().any(|v| v.as_str() == Some("minecraft:lore")));
    if !hidden && let Some(lore) = component(stack, "minecraft:lore") {
        if let Some(lines) = lore.as_array() {
            for (index, line) in lines.iter().enumerate() {
                // ItemLore.LORE_STYLE is DARK_PURPLE + italic in the pinned native version.
                let line =
                    formatted(line, true, Some("dark_purple"), &mut budget).unwrap_or_else(|_| {
                        unsupported = true;
                        component_title_with(line, &translate)
                    });
                if !append(&mut result, &line) {
                    unsupported = true;
                    break;
                }
                if budget.nodes == 0 && index + 1 < lines.len() {
                    unsupported = true;
                    break;
                }
            }
        } else {
            unsupported = true;
        }
    }
    if unsupported {
        let mut end = result
            .len()
            .min(ui::UiLimits::MAX_TEXT_BYTES.saturating_sub(UNSUPPORTED.len() + 1));
        while !result.is_char_boundary(end) {
            end -= 1;
        }
        result.truncate(end);
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(UNSUPPORTED);
    }
    result
}
fn append(out: &mut String, line: &str) -> bool {
    if out.len().saturating_add(line.len()).saturating_add(1)
        > ui::UiLimits::MAX_TEXT_BYTES.saturating_sub(UNSUPPORTED.len() + 1)
    {
        return false;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(line);
    true
}
struct Budget {
    nodes: usize,
    bytes: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            nodes: ui::MAX_TEXT_SPANS,
            bytes: ui::UiLimits::MAX_TEXT_BYTES,
        }
    }
}
fn formatted(
    value: &Value,
    italic: bool,
    color: Option<&str>,
    budget: &mut Budget,
) -> Result<String, ()> {
    let child = owned(value, budget, 0)?;
    let mut parent = NbtCompound::default();
    parent.insert("text", NbtValue::String("".into()));
    parent.insert("italic", NbtValue::Byte(i8::from(italic)));
    if let Some(color) = color {
        parent.insert("color", NbtValue::String(color.into()));
    }
    parent.insert("extra", NbtValue::List(vec![child]));
    text::component_text(&NbtValue::Compound(parent), None).map(|s| s.to_string())
}
fn string(value: &str, budget: &mut Budget) -> Result<Box<str>, ()> {
    budget.bytes = budget.bytes.checked_sub(value.len()).ok_or(())?;
    Ok(value.into())
}
/// Adapts the preserved component JSON representation, not a second network decoder. Both
/// typed wire NBT and persistent resource component forms enter the same text formatter.
fn owned(v: &Value, budget: &mut Budget, depth: usize) -> Result<NbtValue, ()> {
    if depth > world::MAX_NBT_DEPTH {
        return Err(());
    }
    budget.nodes = budget.nodes.checked_sub(1).ok_or(())?;
    Ok(match v {
        Value::String(s) => NbtValue::String(string(s, budget)?),
        Value::Bool(b) => NbtValue::Byte(i8::from(*b)),
        Value::Number(n) => NbtValue::Int(i32::try_from(n.as_i64().ok_or(())?).map_err(|_| ())?),
        Value::Array(a) => NbtValue::List(
            a.iter()
                .map(|v| owned(v, budget, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(m) => {
            if m.len() == 1 {
                if let Some(v) = m.get("String") {
                    return Ok(NbtValue::String(string(v.as_str().ok_or(())?, budget)?));
                }
                if let Some(v) = m.get("Byte") {
                    return Ok(NbtValue::Byte(
                        i8::try_from(v.as_i64().ok_or(())?).map_err(|_| ())?,
                    ));
                }
                if let Some(v) = m.get("List") {
                    return Ok(NbtValue::List(
                        v.as_array()
                            .ok_or(())?
                            .iter()
                            .map(|v| owned(v, budget, depth + 1))
                            .collect::<Result<_, _>>()?,
                    ));
                }
                if let Some(v) = m.get("Compound") {
                    return compound(v.as_object().ok_or(())?, budget, depth);
                }
            }
            return compound(m, budget, depth);
        }
        Value::Null => return Err(()),
    })
}
fn compound(
    m: &serde_json::Map<String, Value>,
    budget: &mut Budget,
    depth: usize,
) -> Result<NbtValue, ()> {
    if depth > world::MAX_NBT_DEPTH {
        return Err(());
    }
    let mut c = NbtCompound::default();
    for (k, v) in m {
        c.insert(string(k, budget)?, owned(v, budget, depth + 1)?);
    }
    Ok(NbtValue::Compound(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn stack() -> Stack {
        Stack {
            item: "minecraft:emerald".into(),
            count: 5,
            added: Default::default(),
            removed: Default::default(),
        }
    }
    #[test]
    fn named_lore_inherits_native_style_and_resets_between_components() {
        let mut s = stack();
        s.added.insert("minecraft:custom_name".into(),json!({"Compound":{"text":{"String":"Shop 名"},"color":{"String":"green"},"bold":{"Byte":1}}}));
        s.added.insert("minecraft:lore".into(),json!([{"String":"Cost: 4 gold"},{"text":"Click to buy","color":"yellow","italic":false}]));
        let out = hover(&s, None);
        assert!(out.contains("§r§a§l§oShop 名"), "{out}");
        assert!(out.contains("§r§5§oCost: 4 gold"), "{out}");
        assert!(out.contains("§r§eClick to buy"), "{out}");
        assert!(!out.contains(UNSUPPORTED));
    }
    #[test]
    fn display_removals_and_malformed_budget_are_explicit() {
        let mut s = stack();
        s.added
            .insert("minecraft:lore".into(), json!(["𐐀", "More"]));
        s.added.insert(
            "minecraft:tooltip_display".into(),
            json!({"hidden_components":["minecraft:lore"]}),
        );
        assert!(!hover(&s, None).contains('𐐀'));
        s.added.insert(
            "minecraft:tooltip_display".into(),
            json!({"hide_tooltip":true}),
        );
        assert_eq!(hover(&s, None), "");
        s.added.remove("minecraft:tooltip_display");
        s.removed.insert("minecraft:lore".into());
        assert!(!hover(&s, None).contains('𐐀'));
        s.removed.clear();
        s.added.insert(
            "minecraft:lore".into(),
            json!(["X".repeat(ui::UiLimits::MAX_TEXT_BYTES + 1)]),
        );
        let out = hover(&s, None);
        assert!(out.contains(UNSUPPORTED));
        assert!(out.len() <= ui::UiLimits::MAX_TEXT_BYTES);
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn changed_item_name_resource_replaces_cached_default_translation() {
        let mut p = super::super::Projection::default();
        p.player = vec![Some(Some(Stack {
            item: "minecraft:emerald".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        }))];
        let names = |word: &str| {
            Language::load(&serde_json::to_vec(&serde_json::json!({"item.test":word})).unwrap())
                .unwrap()
                .with_items([("minecraft:emerald", r#"{"translate":"item.test"}"#)])
                .unwrap()
        };
        let old = Cache::build(&p, Some(&names("Old language")));
        let new = Cache::build(&p, Some(&names("New language")));
        assert_eq!(old.player[0], "Old language");
        assert_eq!(new.player[0], "New language");
    }
    #[test]
    fn presentation_cache_is_a_snapshot_and_new_authority_replaces_its_text() {
        let mut p = super::super::Projection::default();
        let mut item = Stack {
            item: "minecraft:emerald".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        };
        item.added
            .insert("minecraft:lore".into(), serde_json::json!(["Price four"]));
        p.player = vec![Some(Some(item))];
        let old = Cache::build(&p, None);
        assert!(old.player[0].contains("Price four"));
        p.player[0]
            .as_mut()
            .unwrap()
            .as_mut()
            .unwrap()
            .added
            .insert("minecraft:lore".into(), serde_json::json!(["Price six"]));
        let new = Cache::build(&p, None);
        assert!(old.player[0].contains("Price four"));
        assert!(new.player[0].contains("Price six"));
        assert!(!new.player[0].contains("Price four"));
        assert!(
            Cache::build(&super::super::Projection::default(), None)
                .player
                .is_empty()
        );
    }
}
