//! Declaring-method facts from the pinned native runtime. These identify guaranteed
//! base dispatch only; an override is never guessed to PASS or loaded as executable code.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};
#[derive(Debug, thiserror::Error)]
#[error("invalid pinned native use rules: {0}")]
pub struct Error(pub &'static str);
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub item_default: bool,
    pub empty_default: bool,
    /// Native base placement/survival methods and vanilla feature flags. State,
    /// collision, border and entity checks still belong to the placement caller.
    pub placement_default: bool,
    /// Native base replacement rule with a nonreplaceable block material.
    pub placement_surface: bool,
}
#[derive(Debug)]
pub struct Item {
    pub on_owner: String,
    pub air_owner: String,
    pub living_owner: String,
    /// Only exact native BlockItem instances; subclasses keep their own semantics.
    pub placement_block: Option<String>,
}
#[derive(Debug)]
pub struct Table {
    blocks: BTreeMap<String, Block>,
    items: BTreeMap<String, Item>,
    pub border: BorderDefaults,
    unobstructing_entities: BTreeSet<String>,
}
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct BorderDefaults {
    pub center: [f64; 2],
    pub size: f64,
    pub absolute_max: i32,
    pub warning_blocks: i32,
    pub warning_time: i32,
}
#[derive(Deserialize)]
struct Report {
    version: String,
    protocol: i32,
    blocks: Vec<(String, bool, bool, bool, bool)>,
    items: Vec<(String, String, String, String, Option<String>)>,
    border: BorderDefaults,
    entity_count: u64,
    unobstructing_entities: Vec<String>,
    obstruction_owners: Vec<String>,
}
impl Table {
    pub fn load(bytes: &[u8]) -> Result<Self, Error> {
        let target: serde_json::Value =
            serde_json::from_slice(include_bytes!("../../../../java-protocol/data/target.json"))
                .map_err(|_| Error("target"))?;
        let pin = &target["use_rules_report"];
        if Some(bytes.len() as u64) != pin["bytes"].as_u64()
            || Some(format!("{:x}", Sha256::digest(bytes)).as_str()) != pin["sha256"].as_str()
        {
            return Err(Error("digest/length"));
        }
        let report: Report = serde_json::from_slice(bytes).map_err(|_| Error("JSON"))?;
        if report.version
            != target["game_version"]
                .as_str()
                .ok_or(Error("target version"))?
            || Some(i64::from(report.protocol)) != target["protocol"].as_i64()
            || Some(report.blocks.len() as u64) != pin["blocks"].as_u64()
            || Some(report.items.len() as u64) != pin["items"].as_u64()
        {
            return Err(Error("version/registry"));
        }
        if Some(report.entity_count) != target["entity_schema"]["count"].as_u64()
            || report.obstruction_owners.is_empty()
            || report
                .unobstructing_entities
                .iter()
                .any(|n| !n.starts_with("minecraft:"))
        {
            return Err(Error("entity obstruction registry"));
        }
        let unobstructing_entities: BTreeSet<_> =
            report.unobstructing_entities.iter().cloned().collect();
        if unobstructing_entities.len() != report.unobstructing_entities.len() {
            return Err(Error("duplicate entity obstruction identity"));
        }
        let border = report.border;
        if !border.center.iter().all(|v| v.is_finite())
            || !border.size.is_finite()
            || border.size <= 0.
            || border.absolute_max <= 0
        {
            return Err(Error("border defaults"));
        }
        let mut blocks = BTreeMap::new();
        let mut items = BTreeMap::new();
        for (name, item_default, empty_default, placement_default, placement_surface) in
            report.blocks
        {
            if !name.starts_with("minecraft:")
                || blocks
                    .insert(
                        name,
                        Block {
                            item_default,
                            empty_default,
                            placement_default,
                            placement_surface,
                        },
                    )
                    .is_some()
            {
                return Err(Error("block identity"));
            }
        }
        for (name, on_owner, air_owner, living_owner, placement_block) in report.items {
            if placement_block
                .as_ref()
                .is_some_and(|block| !blocks.contains_key(block) || on_owner != "BlockItem")
            {
                return Err(Error("placement block identity"));
            }
            if !name.starts_with("minecraft:")
                || on_owner.is_empty()
                || air_owner.is_empty()
                || living_owner.is_empty()
                || items
                    .insert(
                        name,
                        Item {
                            on_owner,
                            air_owner,
                            living_owner,
                            placement_block,
                        },
                    )
                    .is_some()
            {
                return Err(Error("item identity"));
            }
        }
        Ok(Self {
            blocks,
            items,
            border,
            unobstructing_entities,
        })
    }
    /// Derived from all native writes to Entity.blocksBuilding, not renderer support.
    pub fn never_obstructs(&self, entity: &str) -> bool {
        self.unobstructing_entities.contains(entity)
    }
    pub fn block(&self, name: &str) -> Option<Block> {
        self.blocks.get(name).copied()
    }
    pub fn item(&self, name: &str) -> Option<&Item> {
        self.items.get(name)
    }
}
pub fn builtin() -> Result<&'static Table, &'static Error> {
    static DATA: OnceLock<Result<Table, Error>> = OnceLock::new();
    DATA.get_or_init(|| Table::load(include_bytes!("use-rules.json")))
        .as_ref()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_base_dispatch_and_overrides_stay_distinct() {
        let t = builtin().unwrap();
        let stone = t.block("minecraft:stone").unwrap();
        assert!(stone.item_default && stone.empty_default);
        assert!(stone.placement_default);
        assert_eq!(
            t.item("minecraft:white_wool")
                .unwrap()
                .placement_block
                .as_deref(),
            Some("minecraft:white_wool")
        );
        assert!(!t.block("minecraft:torch").unwrap().placement_default);
        assert!(t.item("minecraft:torch").unwrap().placement_block.is_none());
        assert!(!t.block("minecraft:chest").unwrap().empty_default);
        assert_eq!(t.item("minecraft:bow").unwrap().air_owner, "BowItem");
        assert_eq!(t.item("minecraft:apple").unwrap().on_owner, "Item");
        assert_eq!(t.item("minecraft:potion").unwrap().on_owner, "PotionItem");
        assert_eq!(t.item("minecraft:iron_sword").unwrap().living_owner, "Item");
        assert_eq!(
            t.item("minecraft:name_tag").unwrap().living_owner,
            "NameTagItem"
        );
        assert_eq!(t.item("minecraft:red_dye").unwrap().living_owner, "DyeItem");
        assert!(t.item("fixture:item").is_none());
        assert!(Table::load(b"{}").is_err());
    }
}
