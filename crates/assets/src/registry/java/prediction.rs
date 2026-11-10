//! Hash-pinned native callbacks and fluid replacement IDs for local block prediction.
//! Declaring classes describe verified dispatch; unsupported overrides stay explicit.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::OnceLock};
const BLOCK: &str = "net.minecraft.world.level.block.Block";
const BEHAVIOUR: &str = "net.minecraft.world.level.block.state.BlockBehaviour";
const ITEM: &str = "net.minecraft.world.item.Item";
#[derive(Debug, thiserror::Error)]
#[error("invalid native block prediction facts: {0}")]
pub struct Error(pub &'static str);
#[derive(Debug, Deserialize)]
pub struct Callbacks {
    #[serde(rename = "onPlace")]
    on_place: String,
    #[serde(rename = "setPlacedBy")]
    set_placed_by: String,
    #[serde(rename = "playerWillDestroy")]
    player_will_destroy: String,
    destroy: String,
    #[serde(rename = "updateShape")]
    update_shape: String,
    #[serde(rename = "updateIndirectNeighbourShapes")]
    indirect: String,
    #[serde(rename = "neighborChanged")]
    neighbor_changed: String,
}
#[derive(Debug, Deserialize)]
pub struct Block {
    pub name: String,
    game_master: bool,
    callbacks: Callbacks,
}
impl Block {
    /// Every native neighbor callback is a verified unchanged-state/no-op base method.
    pub fn inert_neighbour(&self) -> bool {
        self.callbacks.update_shape == BEHAVIOUR
            && self.callbacks.indirect == BEHAVIOUR
            && self.callbacks.neighbor_changed == BEHAVIOUR
    }
    /// Only base client mutation hooks are implemented by the direct-cell placement owner.
    pub fn direct_placement(&self) -> bool {
        self.inert_neighbour()
            && self.callbacks.on_place == BEHAVIOUR
            && self.callbacks.set_placed_by == BLOCK
    }
    /// Base destruction has no additional terrain mutation; permission checks remain caller-owned.
    pub fn direct_destruction(&self) -> bool {
        !self.game_master
            && self.inert_neighbour()
            && self.callbacks.player_will_destroy == BLOCK
            && self.callbacks.destroy == BLOCK
    }
}
#[derive(Deserialize)]
struct Report {
    schema: u32,
    version: String,
    protocol: i32,
    source_sha256: String,
    state_count: usize,
    blocks: Vec<Block>,
    items: Vec<(String, String)>,
    runs: Vec<(usize, usize, usize, u32)>,
}
#[derive(Debug)]
pub struct Table {
    blocks: Vec<Block>,
    items: BTreeMap<String, String>,
    runs: Vec<(usize, usize, usize, u32)>,
    states: usize,
}
impl Table {
    pub fn load(bytes: &[u8]) -> Result<Self, Error> {
        let target: serde_json::Value =
            serde_json::from_slice(include_bytes!("../../../../java-protocol/data/target.json"))
                .map_err(|_| Error("target"))?;
        let pin = &target["block_prediction_report"];
        if Some(bytes.len() as u64) != pin["bytes"].as_u64()
            || Some(format!("{:x}", Sha256::digest(bytes)).as_str()) != pin["sha256"].as_str()
        {
            return Err(Error("digest/length"));
        }
        let report: Report = serde_json::from_slice(bytes).map_err(|_| Error("JSON"))?;
        if report.schema != 1
            || Some(report.version.as_str()) != target["game_version"].as_str()
            || Some(i64::from(report.protocol)) != target["protocol"].as_i64()
            || Some(report.source_sha256.as_str()) != pin["source_sha256"].as_str()
            || Some(report.state_count as u64) != pin["states"].as_u64()
            || Some(report.state_count as u64) != target["block_registry_report"]["states"].as_u64()
            || Some(report.blocks.len() as u64) != pin["blocks"].as_u64()
            || Some(report.items.len() as u64) != pin["items"].as_u64()
        {
            return Err(Error("provenance/counts"));
        }
        let mut cursor = 0;
        for &(start, count, block, legacy) in &report.runs {
            cursor = start
                .checked_add(count)
                .filter(|end| {
                    start == cursor
                        && count > 0
                        && *end <= report.state_count
                        && block < report.blocks.len()
                        && (legacy as usize) < report.state_count
                })
                .ok_or(Error("state coverage"))?;
        }
        if cursor != report.state_count {
            return Err(Error("missing states"));
        }
        let item_count = report.items.len();
        let items: BTreeMap<_, _> = report.items.into_iter().collect();
        if items.len() != item_count {
            return Err(Error("duplicate item"));
        }
        Ok(Self {
            blocks: report.blocks,
            items,
            runs: report.runs,
            states: report.state_count,
        })
    }
    /// Returns the native block callback facts and actual fluid-derived replacement state ID.
    pub fn state(&self, id: u32) -> Option<(&Block, u32)> {
        let id = id as usize;
        if id >= self.states {
            return None;
        }
        let index = self
            .runs
            .partition_point(|(start, count, _, _)| start + count <= id);
        let &(_, _, block, legacy) = self.runs.get(index)?;
        Some((self.blocks.get(block)?, legacy))
    }
    pub fn base_item_destruction(&self, item: &str) -> bool {
        self.items.get(item).is_some_and(|owner| owner == ITEM)
    }
}
pub fn builtin() -> Result<&'static Table, &'static Error> {
    static TABLE: OnceLock<Result<Table, Error>> = OnceLock::new();
    TABLE
        .get_or_init(|| Table::load(include_bytes!("prediction.json")))
        .as_ref()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verified_callbacks_and_fluid_replacements_do_not_blanket_accept_overrides_or_force_air() {
        let table = builtin().unwrap();
        let block = |name| {
            table
                .blocks
                .iter()
                .find(|block| block.name == name)
                .unwrap()
        };
        assert!(block("minecraft:white_wool").direct_placement());
        assert!(block("minecraft:stone").direct_destruction());
        assert!(!block("minecraft:sponge").direct_placement());
        assert!(!block("minecraft:oak_door").direct_destruction());
        assert!(!block("minecraft:command_block").direct_destruction());
        let mut waterlogged_slab = false;
        for state in 0..table.states as u32 {
            let (block, legacy) = table.state(state).unwrap();
            let (replacement, _) = table.state(legacy).unwrap();
            if block.name == "minecraft:stone" {
                assert_eq!(replacement.name, "minecraft:air");
            }
            waterlogged_slab |=
                block.name == "minecraft:oak_slab" && replacement.name == "minecraft:water";
        }
        assert!(
            waterlogged_slab,
            "native waterlogged destruction restores water, not air"
        );
        let mut bytes = include_bytes!("prediction.json").to_vec();
        bytes[0] ^= 1;
        assert!(Table::load(&bytes).is_err());
    }
}
