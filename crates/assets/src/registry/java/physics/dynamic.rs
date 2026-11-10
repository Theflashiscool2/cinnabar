//! Target-pinned dynamic shape identities and position-only collision facts.
use super::Bounds;
use crate::registry::java::BlockRegistry;
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

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
        Facts {
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
    let axis = |shift| {
        ((f64::from(((seed >> shift) & 15) as f32 / 15.) - 0.5) * 0.5)
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
    }
}
