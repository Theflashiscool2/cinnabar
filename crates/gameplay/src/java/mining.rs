//! Target-native destroy speed. Inputs come from authoritative components/tags and the
//! sole local simulation; this module never consumes an item or changes a block.
use assets::registry::java::{items::defaults, mining::State};
use inventory::java::Stack;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct Environment {
    pub on_ground: bool,
    pub eyes_in_water: bool,
    pub efficiency: f32,
    pub break_speed: f32,
    pub submerged_speed: f32,
    pub haste: Option<u8>,
    pub fatigue: Option<u8>,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            on_ground: true,
            eyes_in_water: false,
            efficiency: 0.,
            break_speed: 1.,
            submerged_speed: 0.2,
            haste: None,
            fatigue: None,
        }
    }
}
fn component<'a>(stack: &'a Stack, key: &str) -> Option<&'a Value> {
    if stack.removed.contains(key) {
        None
    } else {
        stack
            .added
            .get(key)
            .or_else(|| defaults::component(&stack.item, key))
    }
}
fn matches(
    blocks: &Value,
    state: State,
    name: &str,
    tags: &BTreeMap<String, Vec<i32>>,
) -> Result<bool, &'static str> {
    let tag = |tag: &str| -> bool {
        tags.get(tag)
            .is_some_and(|ids| ids.contains(&(state.block as i32)))
    };
    match blocks {
        Value::String(value) => Ok(value.strip_prefix('#').map_or_else(|| value == name, tag)),
        Value::Array(values) => {
            for v in values {
                if matches(v, state, name, tags)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Value::Object(o) if o.len() == 1 => {
            if let Some(Value::String(name)) = o.get("tag") {
                return Ok(tag(name));
            }
            if let Some(Value::Array(ids)) = o.get("ids") {
                let mut found = false;
                for id in ids {
                    let id = id
                        .as_u64()
                        .filter(|id| *id <= i32::MAX as u64)
                        .ok_or("Invalid tool block ID")?;
                    found |= id == state.block as u64;
                }
                return Ok(found);
            }
            Err("Invalid native tool holder set")
        }
        _ => Err("Invalid native tool holder set"),
    }
}
/// Rule order is independent for speed and drops: an early drop-only exclusion must
/// not suppress the later pickaxe speed rule (Tool#getMiningSpeed/isCorrectForDrops).
fn tool(
    stack: Option<&Stack>,
    state: State,
    name: &str,
    tags: &BTreeMap<String, Vec<i32>>,
) -> Result<(f32, bool), &'static str> {
    let Some(tool) = stack.and_then(|s| component(s, "minecraft:tool")) else {
        return Ok((1., false));
    };
    let mut speed = None;
    let mut correct = None;
    for rule in tool
        .get("rules")
        .and_then(Value::as_array)
        .ok_or("Invalid native tool rules")?
    {
        if !matches(
            rule.get("blocks").ok_or("Missing native tool blocks")?,
            state,
            name,
            tags,
        )? {
            continue;
        }
        if speed.is_none()
            && let Some(v) = rule.get("speed").filter(|v| !v.is_null())
        {
            speed = Some(v.as_f64().ok_or("Invalid native tool speed")? as f32);
        }
        if correct.is_none()
            && let Some(v) = rule.get("correct_for_drops").filter(|v| !v.is_null())
        {
            correct = Some(v.as_bool().ok_or("Invalid native tool drops rule")?);
        }
    }
    let speed = speed.unwrap_or(
        tool.get("default_mining_speed")
            .and_then(Value::as_f64)
            .unwrap_or(1.) as f32,
    );
    if !speed.is_finite() || speed < 0. {
        return Err("Invalid native tool speed");
    }
    Ok((speed, correct.unwrap_or(false)))
}
pub fn creative_allowed(stack: Option<&Stack>) -> Result<bool, &'static str> {
    let Some(tool) = stack.and_then(|s| component(s, "minecraft:tool")) else {
        return Ok(true);
    };
    match tool.get("can_destroy_blocks_in_creative") {
        None | Some(Value::Null) => Ok(true),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err("Invalid native creative tool rule"),
    }
}
/// Float operations deliberately retain the pinned Player#getDestroySpeed and
/// BlockBehaviour#getDestroyProgress order, including airborne and eye-water penalties.
pub fn progress(
    state: State,
    name: &str,
    stack: Option<&Stack>,
    tags: &BTreeMap<String, Vec<i32>>,
    env: Environment,
) -> Result<f32, &'static str> {
    if [env.efficiency, env.break_speed, env.submerged_speed]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.)
    {
        return Err("Invalid native mining environment");
    }
    if state.hardness < 0. {
        return Ok(0.);
    }
    let (mut speed, correct) = tool(stack, state, name, tags)?;
    if speed > 1. {
        speed += env.efficiency;
    }
    if let Some(amp) = env.haste {
        speed *= 1. + (f32::from(amp) + 1.) * 0.2;
    }
    if let Some(amp) = env.fatigue {
        speed *= 0.3_f64.powi(i32::from(amp) + 1) as f32;
    }
    speed *= env.break_speed;
    if env.eyes_in_water {
        speed *= env.submerged_speed;
    }
    if !env.on_ground {
        speed /= 5.;
    }
    Ok(speed
        / state.hardness
        / if !state.requires_tool || correct {
            30.
        } else {
            100.
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn stone() -> State {
        State {
            block: 1,
            hardness: 1.5,
            requires_tool: true,
        }
    }
    fn pick() -> Stack {
        Stack {
            item: "minecraft:iron_pickaxe".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        }
    }
    fn tags() -> BTreeMap<String, Vec<i32>> {
        BTreeMap::from([("minecraft:mineable/pickaxe".into(), vec![1])])
    }
    #[test]
    fn speeds_and_drop_rules_match_independent_native_itemstack_vectors() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/java/mining/vectors.json"
        )))
        .unwrap();
        let facts = assets::registry::java::mining::builtin().unwrap();
        let rows = fixture["vectors"].as_array().unwrap();
        assert_eq!(rows.len(), 36);
        for row in rows {
            let item = Stack {
                item: row["item"].as_str().unwrap().into(),
                count: 1,
                added: Default::default(),
                removed: Default::default(),
            };
            let state = facts.state(row["state"].as_u64().unwrap() as u32).unwrap();
            assert_eq!(state.block as u64, row["block_id"].as_u64().unwrap());
            let tags = row["tags"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| (t.as_str().unwrap().into(), vec![state.block as i32]))
                .collect();
            let (speed, correct) =
                tool(Some(&item), state, row["block"].as_str().unwrap(), &tags).unwrap();
            assert_eq!(
                speed.to_bits() as u64,
                row["speed_bits"].as_u64().unwrap(),
                "{} {}",
                item.item,
                row["block"]
            );
            assert_eq!(
                correct,
                row["correct"].as_bool().unwrap(),
                "{} {}",
                item.item,
                row["block"]
            );
        }
    }
    #[test]
    fn native_timing_drop_exclusion_and_component_removal() {
        let mut pick = pick();
        let tags = tags();
        assert_eq!(
            progress(
                stone(),
                "minecraft:stone",
                None,
                &tags,
                Environment::default()
            )
            .unwrap(),
            1. / 150.
        );
        assert_eq!(
            progress(
                stone(),
                "minecraft:stone",
                Some(&pick),
                &tags,
                Environment::default()
            )
            .unwrap(),
            6. / 45.
        );
        let mut excluded = tags.clone();
        excluded.insert("minecraft:incorrect_for_iron_tool".into(), vec![1]);
        assert_eq!(
            progress(
                stone(),
                "minecraft:stone",
                Some(&pick),
                &excluded,
                Environment::default()
            )
            .unwrap(),
            6. / 150.
        );
        pick.removed.insert("minecraft:tool".into());
        assert_eq!(
            progress(
                stone(),
                "minecraft:stone",
                Some(&pick),
                &tags,
                Environment::default()
            )
            .unwrap(),
            1. / 150.
        );
    }
    #[test]
    fn wire_holders_and_environment_use_native_float_order() {
        let mut pick = pick();
        pick.added.insert("minecraft:tool".into(),json!({"rules":[{"blocks":{"ids":[1]},"speed":6,"correct_for_drops":true}],"default_mining_speed":1}));
        let e = Environment {
            efficiency: 4.,
            haste: Some(1),
            fatigue: Some(1),
            eyes_in_water: true,
            on_ground: false,
            ..Default::default()
        };
        let expected =
            ((((6_f32 + 4.) * (1. + 2. * 0.2)) * (0.3_f64.powi(2) as f32)) * 0.2 / 5.) / 1.5 / 30.;
        assert_eq!(
            progress(stone(), "minecraft:stone", Some(&pick), &tags(), e)
                .unwrap()
                .to_bits(),
            expected.to_bits()
        );
        pick.added.insert("minecraft:tool".into(),json!({"rules":[{"blocks":{"tag":"minecraft:mineable/pickaxe"},"speed":6,"correct_for_drops":true}]}));
        assert_eq!(
            progress(
                stone(),
                "minecraft:stone",
                Some(&pick),
                &tags(),
                Environment::default()
            )
            .unwrap(),
            6. / 45.
        );
    }
    #[test]
    fn unbreakable_instant_and_creative_sword_are_distinct() {
        let mut state = stone();
        state.hardness = -1.;
        assert_eq!(
            progress(state, "x", None, &tags(), Environment::default()),
            Ok(0.)
        );
        state.hardness = 0.;
        assert!(
            progress(state, "x", None, &tags(), Environment::default())
                .unwrap()
                .is_infinite()
        );
        assert!(
            progress(
                state,
                "x",
                None,
                &tags(),
                Environment {
                    break_speed: 0.,
                    ..Default::default()
                }
            )
            .unwrap()
            .is_nan(),
            "native zero hardness/speed is not an instant break"
        );
        let sword = Stack {
            item: "minecraft:diamond_sword".into(),
            ..pick()
        };
        assert_eq!(creative_allowed(Some(&sword)), Ok(false));
        assert_eq!(creative_allowed(None), Ok(true));
    }
}
