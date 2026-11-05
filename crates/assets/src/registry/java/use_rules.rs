//! Declaring-method facts from the pinned native runtime. These identify guaranteed
//! base dispatch only; an override is never guessed to PASS or loaded as executable code.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(Debug, thiserror::Error)]
#[error("invalid pinned native use rules: {0}")]
pub struct Error(pub &'static str);
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub item_default: bool,
    pub empty_default: bool,
}
#[derive(Debug)]
pub struct Item {
    pub on_owner: String,
    pub air_owner: String,
}
#[derive(Debug)]
pub struct Table {
    blocks: BTreeMap<String, Block>,
    items: BTreeMap<String, Item>,
}
#[derive(Deserialize)]
struct Report {
    version: String,
    protocol: i32,
    blocks: Vec<(String, bool, bool)>,
    items: Vec<(String, String, String)>,
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
        let mut blocks = BTreeMap::new();
        let mut items = BTreeMap::new();
        for (name, item_default, empty_default) in report.blocks {
            if !name.starts_with("minecraft:")
                || blocks
                    .insert(
                        name,
                        Block {
                            item_default,
                            empty_default,
                        },
                    )
                    .is_some()
            {
                return Err(Error("block identity"));
            }
        }
        for (name, on_owner, air_owner) in report.items {
            if !name.starts_with("minecraft:")
                || on_owner.is_empty()
                || air_owner.is_empty()
                || items
                    .insert(
                        name,
                        Item {
                            on_owner,
                            air_owner,
                        },
                    )
                    .is_some()
            {
                return Err(Error("item identity"));
            }
        }
        Ok(Self { blocks, items })
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
        assert!(!t.block("minecraft:chest").unwrap().empty_default);
        assert_eq!(t.item("minecraft:bow").unwrap().air_owner, "BowItem");
        assert_eq!(t.item("minecraft:apple").unwrap().on_owner, "Item");
        assert_eq!(t.item("minecraft:potion").unwrap().on_owner, "PotionItem");
        assert!(t.item("fixture:item").is_none());
        assert!(Table::load(b"{}").is_err());
    }
}
