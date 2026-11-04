//! Native class identities and scaffold shapes from the pinned verified runtime.
use super::Bounds;
use crate::registry::java::BlockRegistry;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Ordinary,
    Trapdoor,
    Ladder,
    Scaffolding,
}
#[derive(Deserialize)]
struct Report {
    schema: u32,
    game_version: String,
    protocol: u32,
    blocks_sha256: String,
    states: usize,
    runs: Vec<(usize, Kind)>,
    stable: Vec<[f64; 6]>,
    unstable: Vec<[f64; 6]>,
    below_max_y: f64,
}
struct Facts {
    report: Report,
    kinds: Box<[Kind]>,
    stable: Box<[Bounds]>,
    unstable: Box<[Bounds]>,
}
fn facts() -> &'static Facts {
    static FACTS: OnceLock<Facts> = OnceLock::new();
    FACTS.get_or_init(|| {
        let report: Report =
            serde_json::from_str(include_str!("../../../../data/java-climbing.json"))
                .expect("generated native climbing facts");
        assert_eq!(report.schema, 1);
        assert!(!report.game_version.is_empty() && report.protocol > 0);
        let mut kinds = Vec::with_capacity(report.states);
        for &(count, kind) in &report.runs {
            assert!(count > 0 && kinds.len() + count <= report.states);
            kinds.resize(kinds.len() + count, kind);
        }
        assert_eq!(kinds.len(), report.states);
        let shapes = |rows: &[[f64; 6]]| -> Box<[Bounds]> {
            rows.iter()
                .map(|r| {
                    assert!(r.iter().all(|v| v.is_finite()));
                    assert!((0..3).all(|i| r[i] < r[i + 3]));
                    Bounds {
                        min: [r[0], r[1], r[2]],
                        max: [r[3], r[4], r[5]],
                    }
                })
                .collect()
        };
        assert!(report.below_max_y.is_finite());
        let stable = shapes(&report.stable);
        let unstable = shapes(&report.unstable);
        Facts {
            report,
            kinds: kinds.into(),
            stable,
            unstable,
        }
    })
}
/// Class facts are usable only with the exact native state registry they describe.
pub fn kind(registry: &BlockRegistry, state: u32) -> Option<Kind> {
    let f = facts();
    if registry.len() != f.report.states || !registry.matches_sha256(&f.report.blocks_sha256) {
        return None;
    }
    f.kinds.get(state as usize).copied()
}
/// Native local-space shapes; selection belongs to the entity-context collision query.
pub fn scaffolding_shapes() -> (&'static [Bounds], &'static [Bounds], f64) {
    let f = facts();
    (&f.stable, &f.unstable, f.report.below_max_y)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_climbing_facts_match_the_supported_registry() {
        let target: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../java-protocol/data/target.json"
        ))
        .unwrap();
        let f = facts();
        assert_eq!(
            f.report.game_version,
            target["game_version"].as_str().unwrap()
        );
        assert_eq!(
            f.report.protocol,
            target["protocol"].as_u64().unwrap() as u32
        );
        assert_eq!(
            f.report.blocks_sha256,
            target["block_registry_report"]["sha256"].as_str().unwrap()
        );
        assert_eq!(
            f.kinds.len() as u64,
            target["block_registry_report"]["states"].as_u64().unwrap()
        );
        assert_eq!(f.stable.len(), 7);
        assert_eq!(
            f.unstable.as_ref(),
            &[Bounds {
                min: [0.; 3],
                max: [1., 0.125, 1.]
            }]
        );
        assert_eq!(f.report.below_max_y, 0.);
    }
}
