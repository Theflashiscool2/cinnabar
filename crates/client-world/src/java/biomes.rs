//! Admission of the pinned Java biome appearance schema; renderer consumers see domain facts.
use java_protocol::{nbt::Value, session::facts::RegistryEntry};
use std::collections::BTreeMap;
use world::java::BiomeAppearance;
pub(super) fn read(entries: &[RegistryEntry]) -> Result<Vec<BiomeAppearance>, &'static str> {
    entries
        .iter()
        .map(|entry| {
            let root = compound(&entry.value)?;
            let effects = compound(root.get("effects").ok_or("Java biome effects missing")?)?;
            let float = |key: &str| {
                root.get(key)
                    .ok_or("Java biome climate missing")
                    .and_then(number)
            };
            let modifier = match effects.get("grass_color_modifier") {
                None => None,
                Some(Value::String(s)) => Some(s.clone()),
                _ => return Err("Java biome grass modifier"),
            };
            Ok(BiomeAppearance {
                name: entry.name.clone(),
                temperature: float("temperature")?,
                downfall: float("downfall")?,
                grass_color: effects.get("grass_color").map(color).transpose()?,
                foliage_color: effects.get("foliage_color").map(color).transpose()?,
                water_color: color(
                    effects
                        .get("water_color")
                        .ok_or("Java biome water color missing")?,
                )?,
                grass_modifier: modifier,
            })
        })
        .collect()
}
fn compound(value: &Value) -> Result<&BTreeMap<String, Value>, &'static str> {
    match value {
        Value::Compound(c) => Ok(c),
        _ => Err("Java biome compound"),
    }
}
fn color(value: &Value) -> Result<[u8; 3], &'static str> {
    // 26.3 BiomeSpecialEffects uses ExtraCodecs.STRING_RGB_COLOR: hex, integer or RGB vector.
    let n = match value {
        Value::Int(n) => *n as u32,
        Value::List(values) if values.len() == 3 => {
            let mut channels = [0i32; 3];
            for (channel, value) in channels.iter_mut().zip(values) {
                let scaled = number(value)? * 255.;
                let truncated = scaled as i32;
                *channel = if scaled < (truncated as f32) {
                    truncated.wrapping_sub(1)
                } else {
                    truncated
                };
            }
            ((channels[0] << 16) | (channels[1] << 8) | channels[2]) as u32
        }
        Value::String(s) if s.len() == 7 && s.starts_with('#') => {
            u32::from_str_radix(&s[1..], 16).map_err(|_| "Java biome RGB hex")?
        }
        _ => return Err("Java biome RGB value"),
    };
    Ok([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}
fn number(value: &Value) -> Result<f32, &'static str> {
    let n = match value {
        Value::Byte(n) => *n as f32,
        Value::Short(n) => *n as f32,
        Value::Int(n) => *n as f32,
        Value::Long(n) => *n as f32,
        Value::Float(n) => *n,
        Value::Double(n) => *n as f32,
        _ => return Err("Java biome number"),
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err("Java biome number is not finite")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_rgb_forms_are_validated_without_sign_extension() {
        assert_eq!(
            color(&Value::String("#3f76e4".into())).unwrap(),
            [63, 118, 228]
        );
        assert_eq!(color(&Value::Int(0x3f76e4)).unwrap(), [63, 118, 228]);
        assert_eq!(color(&Value::Int(-1)).unwrap(), [255; 3]);
        assert_eq!(
            color(&Value::List(vec![
                Value::Float(1.),
                Value::Float(0.5),
                Value::Float(0.)
            ]))
            .unwrap(),
            [255, 127, 0]
        );
        assert!(color(&Value::String("#z00000".into())).is_err());
    }
}
