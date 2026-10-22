//! Blockstate variants/multipart selectors for the pinned Java resource schema.
use super::{
    BlockState,
    source::{Error, Identifier},
};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub model: Identifier,
    pub rotations: [u16; 3],
    pub uv_lock: bool,
    pub weight: u32,
}
#[derive(Clone, Debug)]
pub struct Selection {
    /// Each matching multipart part independently selects one weighted variant.
    parts: Vec<Vec<Variant>>,
    pub multipart: bool,
}
impl Selection {
    pub fn parts(&self) -> &[Vec<Variant>] {
        &self.parts
    }
    pub fn read(definition: &Value, state: &BlockState) -> Result<Self, Error> {
        let mut parts = Vec::new();
        let mut multipart = false;
        if let Some(variants) = definition.get("variants") {
            let variants = variants
                .as_object()
                .ok_or_else(|| invalid("variants must be an object"))?;
            for (condition, model) in variants {
                let matches = if condition.is_empty() {
                    true
                } else {
                    condition
                        .split(',')
                        .map(|term| {
                            let (key, value) = term
                                .split_once('=')
                                .ok_or_else(|| invalid("variant property condition"))?;
                            Ok(state
                                .properties
                                .get(key)
                                .is_some_and(|actual| actual == value))
                        })
                        .collect::<Result<Vec<bool>, Error>>()?
                        .into_iter()
                        .all(|v| v)
                };
                if matches {
                    parts.push(variants_read(model)?);
                }
            }
            if parts.len() > 1 {
                return Err(invalid("overlapping variants"));
            }
        }
        if let Some(selectors) = definition.get("multipart") {
            if definition.get("variants").is_some() {
                return Err(invalid("mixed variants and multipart"));
            }
            multipart = true;
            for selector in selectors
                .as_array()
                .ok_or_else(|| invalid("multipart must be a list"))?
            {
                if selector
                    .get("when")
                    .map(|condition| condition_matches(condition, &state.properties, 0))
                    .transpose()?
                    .unwrap_or(true)
                {
                    parts.push(variants_read(
                        selector
                            .get("apply")
                            .ok_or_else(|| invalid("missing multipart apply"))?,
                    )?);
                }
            }
        }
        if !multipart && parts.is_empty() {
            return Err(invalid("no matching blockstate variant"));
        }
        Ok(Self { parts, multipart })
    }
    /// Uses Java's positional seed and legacy RNG; multipart children share a derived seed.
    pub fn choose(&self, position: [i32; 3]) -> Vec<&Variant> {
        let mut random = Random::new(position_seed(position));
        let multipart_seed = if self.multipart {
            Some(random.long())
        } else {
            None
        };
        self.parts
            .iter()
            .map(|part| {
                if let Some(seed) = multipart_seed {
                    random = Random::new(seed);
                }
                let total = part.iter().map(|v| v.weight).sum();
                let mut index = random.bounded(total);
                part.iter()
                    .find(|v| {
                        if index < v.weight {
                            true
                        } else {
                            index -= v.weight;
                            false
                        }
                    })
                    .unwrap()
            })
            .collect()
    }
}
fn invalid(message: &str) -> Error {
    Error::Model(message.into())
}
fn variants_read(value: &Value) -> Result<Vec<Variant>, Error> {
    let elements: Vec<&Value> = if let Some(a) = value.as_array() {
        a.iter().collect()
    } else {
        vec![value]
    };
    if elements.is_empty() {
        return Err(invalid("empty variant list"));
    }
    let mut variants = Vec::with_capacity(elements.len());
    let mut total = 0u32;
    for value in elements {
        let model = Identifier::parse(
            value
                .get("model")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("missing model identifier"))?,
        )?;
        let mut rotations = [0; 3];
        for (index, key) in ["x", "y", "z"].iter().enumerate() {
            if let Some(n) = value.get(key) {
                let n = n
                    .as_i64()
                    .filter(|n| i32::try_from(*n).is_ok())
                    .ok_or_else(|| invalid("model rotation is not a quadrant"))?
                    .rem_euclid(360);
                if ![0, 90, 180, 270].contains(&n) {
                    return Err(invalid("model rotation is not a quadrant"));
                }
                rotations[index] = n as u16;
            }
        }
        let weight = value
            .get("weight")
            .map(|n| n.as_u64().ok_or_else(|| invalid("variant weight")))
            .transpose()?
            .unwrap_or(1);
        if weight == 0 || weight > i32::MAX as u64 {
            return Err(invalid("variant weight"));
        }
        total = total
            .checked_add(weight as u32)
            .filter(|n| *n <= i32::MAX as u32)
            .ok_or_else(|| invalid("variant total weight"))?;
        let uv_lock = value
            .get("uvlock")
            .map(|b| b.as_bool().ok_or_else(|| invalid("uvlock must be boolean")))
            .transpose()?
            .unwrap_or(false);
        variants.push(Variant {
            model,
            rotations,
            uv_lock,
            weight: weight as u32,
        });
    }
    Ok(variants)
}
fn condition_matches(
    value: &Value,
    properties: &BTreeMap<String, String>,
    depth: usize,
) -> Result<bool, Error> {
    if depth > 64 {
        return Err(invalid("multipart condition depth budget"));
    }
    let map = value
        .as_object()
        .ok_or_else(|| invalid("multipart condition must be an object"))?;
    if map.is_empty() {
        return Err(invalid("empty multipart condition"));
    }
    if let Some((operation, children)) = map
        .iter()
        .find(|(key, _)| key.as_str() == "OR" || key.as_str() == "AND")
    {
        if map.len() != 1 {
            return Err(invalid("mixed multipart operators/properties"));
        }
        let values = children
            .as_array()
            .ok_or_else(|| invalid("multipart operator needs a list"))?
            .iter()
            .map(|c| condition_matches(c, properties, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(if operation == "OR" {
            values.iter().any(|b| *b)
        } else {
            values.iter().all(|b| *b)
        });
    }
    let mut result = true;
    for (key, terms) in map {
        let terms = match terms {
            Value::String(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) if n.is_i64() => n.to_string(),
            _ => return Err(invalid("multipart property terms")),
        };
        let (invert, terms) = terms
            .strip_prefix('!')
            .map_or((false, terms.as_str()), |s| (true, s));
        if terms.split('|').any(str::is_empty) {
            return Err(invalid("empty multipart property term"));
        }
        let actual = properties
            .get(key)
            .ok_or_else(|| invalid("unknown multipart property"))?;
        let contains = terms.split('|').any(|t| t == actual);
        result &= contains != invert;
    }
    Ok(result)
}
/// Java performs the X multiply as an overflowing int before widening it to long.
fn position_seed([x, y, z]: [i32; 3]) -> i64 {
    let seed = i64::from(x.wrapping_mul(3_129_871))
        ^ i64::from(z).wrapping_mul(116_129_781)
        ^ i64::from(y);
    seed.wrapping_mul(seed)
        .wrapping_mul(42_317_861)
        .wrapping_add(seed.wrapping_mul(11))
        >> 16
}
struct Random(u64);
impl Random {
    fn new(seed: i64) -> Self {
        Self((seed as u64 ^ 0x5deece66d) & ((1 << 48) - 1))
    }
    fn bits(&mut self, n: u32) -> u32 {
        self.0 = self.0.wrapping_mul(0x5deece66d).wrapping_add(11) & ((1 << 48) - 1);
        (self.0 >> (48 - n)) as u32
    }
    fn long(&mut self) -> i64 {
        (i64::from(self.bits(32) as i32) << 32).wrapping_add(i64::from(self.bits(32) as i32))
    }
    fn bounded(&mut self, bound: u32) -> u32 {
        if bound.is_power_of_two() {
            return ((u64::from(bound) * u64::from(self.bits(31))) >> 31) as u32;
        }
        loop {
            let bits = self.bits(31);
            let value = bits % bound;
            if (bits.wrapping_sub(value).wrapping_add(bound - 1) as i32) >= 0 {
                return value;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> BlockState {
        BlockState {
            name: "test:panel".into(),
            properties: BTreeMap::from([
                ("facing".into(), "west".into()),
                ("lit".into(), "false".into()),
            ]),
            is_default: true,
        }
    }
    #[test]
    fn variants_match_all_properties_and_preserve_all_three_rotations() {
        let value = serde_json::json!({"variants":{"facing=east":{"model":"test:east"},"facing=west,lit=false":{"model":"test:west","x":90,"y":180,"z":-90,"uvlock":true}}});
        let s = Selection::read(&value, &state()).unwrap();
        let chosen = s.choose([-3, 20, 5]);
        assert_eq!(chosen[0].rotations, [90, 180, 270]);
        assert!(chosen[0].uv_lock);
    }
    #[test]
    fn multipart_supports_boolean_terms_negation_and_nested_logic() {
        let value = serde_json::json!({"multipart":[{"when":{"AND":[{"facing":"!north|south"},{"lit":false}]},"apply":{"model":"test:match"}},{"when":{"OR":[{"facing":"east"},{"lit":true}]},"apply":{"model":"test:miss"}}]});
        let s = Selection::read(&value, &state()).unwrap();
        assert_eq!(s.choose([0, 0, 0])[0].model.to_string(), "test:match");
        assert_eq!(s.parts.len(), 1);
    }
    #[test]
    fn invalid_or_ambiguous_selectors_fail() {
        for value in [
            serde_json::json!({"variants":{"":{"model":"test:a"},"facing=west":{"model":"test:b"}}}),
            serde_json::json!({"variants":{"":[]}}),
            serde_json::json!({"variants":{"":{"model":"test:a","weight":0}}}),
        ] {
            assert!(Selection::read(&value, &state()).is_err());
        }
    }
    #[test]
    fn legacy_random_matches_java_util_random_seed_zero_vector() {
        let mut random = Random::new(0);
        assert_eq!(
            (0..5).map(|_| random.bounded(100)).collect::<Vec<_>>(),
            [60, 48, 29, 47, 15]
        );
        assert_eq!(Random::new(0).long(), -4962768465676381896);
    }
}
