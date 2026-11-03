//! Native component codec hashes for click reconciliation, not authoritative item mutation.
use super::hash_ops::{HashCode, HashOps as H};
use serde_json::Value;

/// Unimplemented codec forms are deliberately absent from the prediction map. Vanilla compares
/// that map with its authoritative stack and sends a correction, rather than trusting the client.
pub(super) fn component(name: &str, value: &Value) -> Option<i32> {
    let hash = match name {
        "minecraft:max_stack_size"
        | "minecraft:max_damage"
        | "minecraft:damage"
        | "minecraft:repair_cost"
        | "minecraft:map_id"
        | "minecraft:dyed_color" => H::create_int(i32::try_from(value.as_i64()?).ok()?),
        "minecraft:enchantment_glint_override" => H::create_boolean(value.as_bool()?),
        "minecraft:unbreakable" if value.is_null() => H::empty_map(),
        "minecraft:item_model" | "minecraft:tooltip_style" => H::create_string(value.as_str()?),
        "minecraft:custom_name" | "minecraft:item_name" => nbt(value, true, 0)?,
        "minecraft:custom_data" => nbt(value, false, 0)?,
        "minecraft:lore" => H::create_list(
            value
                .as_array()?
                .iter()
                .map(|v| nbt(v, true, 0))
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => return None,
    };
    Some(hash.as_int())
}
fn nbt(value: &Value, text: bool, depth: usize) -> Option<HashCode> {
    if depth > 64 {
        return None;
    }
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    let (kind, v) = object.iter().next()?;
    Some(match kind.as_str() {
        "Byte" => H::create_byte(i8::try_from(v.as_i64()?).ok()?),
        "Short" => H::create_short(i16::try_from(v.as_i64()?).ok()?),
        "Int" => H::create_int(i32::try_from(v.as_i64()?).ok()?),
        "Long" => H::create_long(v.as_i64()?),
        "Float" => H::create_f32(v.as_f64()? as f32),
        "Double" => H::create_double(v.as_f64()?.to_bits()),
        "String" => H::create_string(v.as_str()?),
        "List" => H::create_list(
            v.as_array()?
                .iter()
                .map(|v| nbt(v, text, depth + 1))
                .collect::<Option<Vec<_>>>()?,
        ),
        "Compound" => H::create_map(
            v.as_object()?
                .iter()
                .map(|(key, value)| {
                    let boolean = text
                        && [
                            "bold",
                            "italic",
                            "underlined",
                            "strikethrough",
                            "obfuscated",
                            "interpret",
                            "plain",
                        ]
                        .contains(&key.as_str());
                    let hash = if boolean && value.get("Byte").is_some() {
                        H::create_boolean(value.get("Byte")?.as_i64()? != 0)
                    } else {
                        nbt(value, text, depth + 1)?
                    };
                    Some((H::create_string(key), hash))
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "Bytes" => H::create_byte_list(
            &v.as_array()?
                .iter()
                .map(|v| {
                    v.as_i64()
                        .and_then(|n| i8::try_from(n).ok())
                        .map(|n| n as u8)
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "Ints" => H::create_int_list(
            &v.as_array()?
                .iter()
                .map(|v| v.as_i64().and_then(|n| i32::try_from(n).ok()))
                .collect::<Option<Vec<_>>>()?,
        ),
        "Longs" => H::create_long_list(
            &v.as_array()?
                .iter()
                .map(Value::as_i64)
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn official_text_hashes_and_unknown_codec_forms_are_explicit() {
        assert_eq!(
            component("minecraft:custom_name", &json!({"String":"Hi"})),
            Some(485089409)
        );
        assert_eq!(
            component("minecraft:item_model", &json!("minecraft:stick")),
            Some(-1153337500)
        );
        assert_eq!(
            component("minecraft:enchantment_glint_override", &json!(true)),
            Some(-1019818302)
        );
        assert_eq!(
            component("minecraft:unbreakable", &Value::Null),
            Some(-982207288)
        );
        assert_eq!(
            component(
                "minecraft:custom_name",
                &json!({"Compound": {
                    "text":{"String":"Hi"}, "bold":{"Byte":1}, "color":{"String":"red"}
                }})
            ),
            Some(-204970011)
        );
        assert_eq!(
            component(
                "minecraft:lore",
                &json!([
                    {"String":"one"}, {"Compound":{"text":{"String":"two"},"underlined":{"Byte":1}}}
                ])
            ),
            Some(931033850)
        );
        assert_eq!(component("minecraft:lore", &json!([])), Some(-1978007022));
        assert_eq!(component("minecraft:attribute_modifiers", &json!([])), None);
        assert_eq!(
            component("minecraft:custom_name", &json!("not typed NBT")),
            None
        );
    }
}
