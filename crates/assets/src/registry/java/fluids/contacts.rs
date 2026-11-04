//! Native sturdy faces for fluid currents, deliberately distinct from visual occlusion.
use super::super::BlockRegistry;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Report {
    schema: u32,
    game_version: String,
    protocol: u32,
    blocks_sha256: String,
    fast_lava_default: bool,
    states: usize,
    runs: Vec<(usize, u8)>,
}
pub fn fast_lava_default() -> bool {
    facts().report.fast_lava_default
}
struct Facts {
    report: Report,
    masks: Box<[u8]>,
}
fn facts() -> &'static Facts {
    static FACTS: OnceLock<Facts> = OnceLock::new();
    FACTS.get_or_init(|| {
        let report: Report =
            serde_json::from_str(include_str!("../../../../data/java-fluid-contacts.json"))
                .expect("generated native fluid contact facts");
        assert_eq!(report.schema, 1);
        assert!(!report.game_version.is_empty() && report.protocol > 0);
        let mut masks = Vec::with_capacity(report.states);
        for &(count, mask) in &report.runs {
            assert!(count > 0 && mask & 64 == 0 && masks.len() + count <= report.states);
            masks.resize(masks.len() + count, mask);
        }
        assert_eq!(masks.len(), report.states);
        Facts {
            report,
            masks: masks.into(),
        }
    })
}
/// Returns only facts matching this native block registry; a different registry is not air.
pub fn mask(registry: &BlockRegistry, state: u32) -> Option<u8> {
    let facts = facts();
    if registry.len() != facts.report.states {
        return None;
    }
    // Compare bytes without formatting/allocating on the per-cell query path.
    let digest = registry.digest();
    let hash = facts.report.blocks_sha256.as_bytes();
    const HEX: &[u8] = b"0123456789abcdef";
    if hash.len() != 64
        || digest.iter().enumerate().any(|(i, n)| {
            hash[i * 2] != HEX[(n >> 4) as usize] || hash[i * 2 + 1] != HEX[(n & 15) as usize]
        })
    {
        return None;
    }
    facts.masks.get(state as usize).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_contact_registry_matches_the_supported_version() {
        let pin: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../java-protocol/data/target.json"
        ))
        .unwrap();
        let report = &facts().report;
        assert_eq!(report.game_version, pin["game_version"].as_str().unwrap());
        assert_eq!(
            u64::from(report.protocol),
            pin["protocol"].as_u64().unwrap()
        );
        assert_eq!(
            report.blocks_sha256,
            pin["block_registry_report"]["sha256"].as_str().unwrap()
        );
        assert_eq!(
            report.states as u64,
            pin["block_registry_report"]["states"].as_u64().unwrap()
        );
    }
}
