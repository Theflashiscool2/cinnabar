//! Target-pinned dynamic shape identities and position-only collision facts.
use super::Bounds;
use crate::registry::java::BlockRegistry;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Offset,
    Shulker,
    Moving,
    PowderSnow,
}
#[derive(Deserialize)]
struct Entry {
    kind: Kind,
    base: Vec<[f64; 6]>,
    #[serde(default)]
    max_offset: f32,
    block_id: Option<u32>,
    facing: Option<u8>,
}
#[derive(Deserialize)]
struct Report {
    schema: u32,
    game_version: String,
    protocol: u32,
    blocks_sha256: String,
    states: usize,
    dynamic: BTreeMap<u32, Entry>,
    powder_falling: Vec<[f64; 6]>,
    powder_standing: Vec<[f64; 6]>,
}
struct Facts {
    report: Report,
    base: BTreeMap<u32, Box<[Bounds]>>,
    shulker_states: BTreeMap<u32, Arc<[u32]>>,
    powder_falling: Box<[Bounds]>,
    powder_standing: Box<[Bounds]>,
}
fn bounds(rows: &[[f64; 6]]) -> Box<[Bounds]> {
    rows.iter()
        .map(|r| Bounds {
            min: [r[0], r[1], r[2]],
            max: [r[3], r[4], r[5]],
        })
        .collect()
}
fn facts() -> &'static Facts {
    static FACTS: OnceLock<Facts> = OnceLock::new();
    FACTS.get_or_init(|| {
        let report: Report =
            serde_json::from_str(include_str!("../../../../data/java-dynamic-collision.json"))
                .expect("generated native dynamic collision facts");
        assert_eq!(report.schema, 1);
        assert!(!report.game_version.is_empty() && report.protocol > 0);
        let mut shulker_states: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for (&id, entry) in &report.dynamic {
            if entry.kind == Kind::Shulker {
                assert!(entry.facing.is_some_and(|facing| facing < 6));
                shulker_states
                    .entry(entry.block_id.expect("native shulker block identity"))
                    .or_default()
                    .push(id);
            }
        }
        Facts {
            shulker_states: shulker_states
                .into_iter()
                .map(|(block, states)| (block, Arc::from(states)))
                .collect(),
            base: report
                .dynamic
                .iter()
                .map(|(id, e)| (*id, bounds(&e.base)))
                .collect(),
            powder_falling: bounds(&report.powder_falling),
            powder_standing: bounds(&report.powder_standing),
            report,
        }
    })
}
fn entry(registry: &BlockRegistry, id: u32) -> Option<&'static Entry> {
    let f = facts();
    if registry.len() != f.report.states || !registry.matches_sha256(&f.report.blocks_sha256) {
        return None;
    }
    f.report.dynamic.get(&id)
}
/// Native class classification is admitted only for the exact report registry.
pub fn kind(registry: &BlockRegistry, id: u32) -> Option<Kind> {
    Some(entry(registry, id)?.kind)
}

/// Compile-target shulker identity for synchronous events before runtime graphics admission.
/// The report's complete registry identity is checked against the protocol target in tests.
pub fn shulker_identity(id: u32) -> Option<(u32, u8)> {
    let entry = facts().report.dynamic.get(&id)?;
    (entry.kind == Kind::Shulker).then_some((entry.block_id?, entry.facing?))
}

/// All facing variants of a native box share one block entity across state updates.
pub fn shulker_states(block: u32) -> Option<Arc<[u32]>> {
    facts().shulker_states.get(&block).cloned()
}

/// Native defaults queried with a BlockGetter that contains no block entity.
pub fn missing_block_entity(registry: &BlockRegistry, id: u32) -> Option<&'static [Bounds]> {
    match entry(registry, id)?.kind {
        Kind::Shulker | Kind::Moving => Some(facts().base.get(&id)?),
        _ => None,
    }
}

/// Speleothem and bamboo collision shapes depend only on state and signed X/Z coordinates.
pub fn offset_shape(registry: &BlockRegistry, id: u32, position: [i32; 3]) -> Option<Vec<Bounds>> {
    let entry = entry(registry, id)?;
    if entry.kind != Kind::Offset {
        return None;
    }
    let seed = i64::from(position[0].wrapping_mul(3_129_871))
        ^ i64::from(position[2]).wrapping_mul(116_129_781);
    let seed = seed
        .wrapping_mul(seed)
        .wrapping_mul(42_317_861)
        .wrapping_add(seed.wrapping_mul(11))
        >> 16;
    let axis = |shift: u32| {
        ((f64::from(((seed >> shift) & 15_i64) as f32 / 15.) - 0.5) * 0.5)
            .clamp(-f64::from(entry.max_offset), f64::from(entry.max_offset))
    };
    let delta = [axis(0), 0., axis(8)];
    Some(
        facts()
            .base
            .get(&id)?
            .iter()
            .map(|b| Bounds {
                min: std::array::from_fn(|i| b.min[i] + delta[i]),
                max: std::array::from_fn(|i| b.max[i] + delta[i]),
            })
            .collect(),
    )
}

/// PowderSnowBlock prioritizes the falling shape only above this native distance.
pub const POWDER_FALL_DISTANCE: f64 = 2.5;

/// The native full-block support test used only after walking-on-snow eligibility.
pub fn powder_standing_context(position: [i32; 3], bottom: f64, descending: bool) -> bool {
    bottom > f64::from(position[1]) + 1. - f64::from(1.0e-5_f32) && !descending
}

/// PowderSnowBlock's living-player branch, including falling priority over walkable equipment.
pub fn powder_shape(
    registry: &BlockRegistry,
    id: u32,
    position: [i32; 3],
    bottom: f64,
    descending: bool,
    fall_distance: f64,
    walkable: bool,
) -> Option<&'static [Bounds]> {
    if entry(registry, id)?.kind != Kind::PowderSnow {
        return None;
    }
    Some(if fall_distance > POWDER_FALL_DISTANCE {
        &facts().powder_falling
    } else if walkable && powder_standing_context(position, bottom, descending) {
        &facts().powder_standing
    } else {
        &[]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dynamic_collision_facts_match_the_supported_target() {
        let target: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../java-protocol/data/target.json"
        ))
        .unwrap();
        let f = facts();
        assert_eq!(f.report.game_version, target["game_version"]);
        assert_eq!(
            f.report.protocol,
            target["protocol"].as_u64().unwrap() as u32
        );
        assert_eq!(
            f.report.states,
            target["block_registry_report"]["states"].as_u64().unwrap() as usize
        );
        assert_eq!(
            f.report.blocks_sha256,
            target["block_registry_report"]["sha256"]
        );
        assert_eq!(
            f.report
                .dynamic
                .values()
                .filter(|e| e.kind == Kind::Offset)
                .count(),
            52
        );
        assert_eq!(
            f.report
                .dynamic
                .values()
                .filter(|e| e.kind == Kind::Shulker)
                .count(),
            102
        );
        assert_eq!(f.shulker_states.len(), 17);
        for states in f.shulker_states.values() {
            assert_eq!(states.len(), 6);
            let mut facings: Vec<_> = states
                .iter()
                .map(|id| shulker_identity(*id).unwrap().1)
                .collect();
            facings.sort_unstable();
            assert_eq!(facings, [0, 1, 2, 3, 4, 5]);
        }
    }
}
