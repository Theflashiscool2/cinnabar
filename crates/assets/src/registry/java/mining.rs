//! Target-native breaking properties, generated from every official block state.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub block: u32,
    pub hardness: f32,
    pub requires_tool: bool,
}
#[derive(Debug, thiserror::Error)]
#[error("invalid pinned Java mining facts: {0}")]
pub struct Error(pub &'static str);
#[derive(Deserialize)]
struct Report {
    version: String,
    protocol: i32,
    state_count: usize,
    block_count: usize,
    runs: Vec<(usize, usize, u32, u32, bool)>,
}
#[derive(Deserialize)]
struct Target {
    game_version: String,
    protocol: i32,
    block_registry_report: Registry,
    mining_report: Pin,
}
#[derive(Deserialize)]
struct Registry {
    states: usize,
}
#[derive(Deserialize)]
struct Pin {
    sha256: String,
    bytes: usize,
    blocks: usize,
}
#[derive(Debug)]
pub struct Table {
    runs: Vec<(usize, usize, State)>,
    states: usize,
}
impl Table {
    /// Validates full coverage, exact float bits and target provenance before publication.
    pub fn load(bytes: &[u8]) -> Result<Self, Error> {
        let target: Target =
            serde_json::from_str(include_str!("../../../../java-protocol/data/target.json"))
                .map_err(|_| Error("target metadata"))?;
        if bytes.len() != target.mining_report.bytes
            || format!("{:x}", Sha256::digest(bytes)) != target.mining_report.sha256
        {
            return Err(Error("digest/length"));
        }
        let report: Report = serde_json::from_slice(bytes).map_err(|_| Error("JSON"))?;
        Self::checked(report, &target)
    }
    fn checked(report: Report, target: &Target) -> Result<Self, Error> {
        if report.version != target.game_version
            || report.protocol != target.protocol
            || report.state_count != target.block_registry_report.states
            || report.block_count != target.mining_report.blocks
        {
            return Err(Error("version/registry"));
        }
        let mut cursor = 0;
        let mut runs = Vec::with_capacity(report.runs.len());
        for (start, count, block, bits, requires_tool) in report.runs {
            let end = start.checked_add(count).ok_or(Error("range overflow"))?;
            let hardness = f32::from_bits(bits);
            if start != cursor
                || count == 0
                || end > report.state_count
                || block as usize >= report.block_count
                || !hardness.is_finite()
            {
                return Err(Error("state coverage/properties"));
            }
            runs.push((
                start,
                end,
                State {
                    block,
                    hardness,
                    requires_tool,
                },
            ));
            cursor = end;
        }
        if cursor != report.state_count {
            return Err(Error("incomplete states"));
        }
        Ok(Self {
            runs,
            states: cursor,
        })
    }
    pub fn state(&self, id: u32) -> Option<State> {
        let id = id as usize;
        if id >= self.states {
            return None;
        }
        self.runs
            .get(self.runs.partition_point(|(_, end, _)| *end <= id))
            .map(|r| r.2)
    }
    pub fn len(&self) -> usize {
        self.states
    }
    pub fn is_empty(&self) -> bool {
        self.states == 0
    }
}
/// Immutable generated facts share the version pin and need no user-side runtime export.
pub fn builtin() -> Result<&'static Table, &'static Error> {
    static TABLE: OnceLock<Result<Table, Error>> = OnceLock::new();
    TABLE
        .get_or_init(|| Table::load(include_bytes!("mining.json")))
        .as_ref()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_registry_has_every_state_and_preserves_unbreakable_and_zero_values() {
        let table = builtin().unwrap();
        let target: Target =
            serde_json::from_str(include_str!("../../../../java-protocol/data/target.json"))
                .unwrap();
        assert_eq!(table.len(), target.block_registry_report.states);
        assert_eq!(table.state(0).unwrap().hardness, 0.);
        assert!((0..table.len() as u32).all(|id| table.state(id).is_some()));
        assert!((0..table.len() as u32).any(|id| table.state(id).unwrap().hardness < 0.));
        assert!(table.state(table.len() as u32).is_none());
        assert!(Table::load(b"{}").is_err());
    }
    #[test]
    fn incomplete_overlapping_nonfinite_or_wrong_version_never_publish() {
        let target: Target =
            serde_json::from_str(include_str!("../../../../java-protocol/data/target.json"))
                .unwrap();
        for mode in 0..4 {
            let mut r: Report = serde_json::from_slice(include_bytes!("mining.json")).unwrap();
            match mode {
                0 => {
                    r.runs.remove(0);
                }
                1 => r.runs[1].0 = 0,
                2 => r.runs[0].3 = f32::NAN.to_bits(),
                _ => r.protocol += 1,
            }
            assert!(Table::checked(r, &target).is_err());
        }
    }
}
