//! Version-pinned property parsers and Comparable order for Adventure permissions.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Permission {
    Break,
    PlaceOn,
}
impl Permission {
    pub fn component(self) -> &'static str {
        match self {
            Self::Break => "minecraft:can_break",
            Self::PlaceOn => "minecraft:can_place_on",
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Integer,
    Boolean,
    Enum,
}
#[derive(Debug, Deserialize)]
pub struct Property {
    kind: Kind,
    #[serde(rename = "class")]
    _class: String,
    values: Vec<String>,
}
#[derive(Debug, Deserialize)]
pub struct Block {
    pub name: String,
    pub block_entity: bool,
    pub properties: BTreeMap<String, Property>,
}
#[derive(Debug, thiserror::Error)]
#[error("invalid pinned Adventure facts: {0}")]
pub struct Error(pub &'static str);
#[derive(Deserialize)]
struct Report {
    schema: u32,
    version: String,
    protocol: i32,
    source_sha256: String,
    state_count: usize,
    blocks: Vec<Block>,
    integer_digit_starts: Vec<u16>,
    default_namespace: String,
    namespace_characters: String,
    path_characters: String,
}
#[derive(Debug)]
pub struct Table {
    blocks: Vec<Block>,
    names: BTreeMap<String, usize>,
    digits: Vec<u16>,
    default_namespace: String,
    namespace_characters: String,
    path_characters: String,
}
impl Table {
    pub fn load(bytes: &[u8]) -> Result<Self, Error> {
        let target: serde_json::Value =
            serde_json::from_slice(include_bytes!("../../../../java-protocol/data/target.json"))
                .map_err(|_| Error("target"))?;
        let pin = &target["adventure_report"];
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
        {
            return Err(Error("provenance/counts"));
        }
        let names: BTreeMap<_, _> = report
            .blocks
            .iter()
            .enumerate()
            .map(|(id, block)| (block.name.clone(), id))
            .collect();
        if names.len() != report.blocks.len()
            || report
                .integer_digit_starts
                .windows(2)
                .any(|pair| u32::from(pair[0]) + 9 >= u32::from(pair[1]))
        {
            return Err(Error("duplicate names/digits"));
        }
        for block in &report.blocks {
            for property in block.properties.values() {
                let distinct: std::collections::BTreeSet<_> = property.values.iter().collect();
                if property.values.is_empty() || distinct.len() != property.values.len() {
                    return Err(Error("property domain"));
                }
            }
        }
        Ok(Self {
            blocks: report.blocks,
            names,
            digits: report.integer_digit_starts,
            default_namespace: report.default_namespace,
            namespace_characters: report.namespace_characters,
            path_characters: report.path_characters,
        })
    }
    pub fn block(&self, name: &str) -> Option<(u32, &Block)> {
        let id = *self.names.get(name)?;
        Some((u32::try_from(id).ok()?, &self.blocks[id]))
    }
    /// Native Identifier.parse normalization for logical registry/tag names. These
    /// names are not filesystem paths, so valid repeated or dot path segments remain valid.
    pub fn identifier<'a>(&self, value: &'a str) -> Option<std::borrow::Cow<'a, str>> {
        let (namespace, path, explicit) = match value.split_once(':') {
            Some((namespace, path)) if !namespace.is_empty() => (namespace, path, true),
            Some((_, path)) => (self.default_namespace.as_str(), path, false),
            None => (self.default_namespace.as_str(), value, false),
        };
        if namespace == ".."
            || !namespace
                .bytes()
                .all(|byte| self.namespace_characters.as_bytes().contains(&byte))
            || !path
                .bytes()
                .all(|byte| self.path_characters.as_bytes().contains(&byte))
        {
            return None;
        }
        Some(if explicit {
            std::borrow::Cow::Borrowed(value)
        } else {
            std::borrow::Cow::Owned(format!("{namespace}:{path}"))
        })
    }
    /// Native Integer.parseInt reads UTF-16 char digits, not ASCII-only or code-point digits.
    fn integer(&self, value: &str) -> Option<i32> {
        let mut units = value.encode_utf16();
        let first = units.next()?;
        let negative = first == u16::from(b'-');
        let signed = negative || first == u16::from(b'+');
        let mut magnitude = 0_i64;
        let mut count = 0;
        for unit in (!signed).then_some(first).into_iter().chain(units) {
            let digit = self
                .digits
                .iter()
                .find_map(|start| unit.checked_sub(*start).filter(|digit| *digit < 10))?;
            magnitude = magnitude.checked_mul(10)?.checked_add(i64::from(digit))?;
            if magnitude > i64::from(i32::MAX) + i64::from(negative) {
                return None;
            }
            count += 1;
        }
        if count == 0 {
            return None;
        }
        i32::try_from(if negative { -magnitude } else { magnitude }).ok()
    }
    /// Resolves a property string exactly as native Property.getValue, then its Comparable rank.
    pub fn property_index(&self, property: &Property, value: &str) -> Option<usize> {
        match property.kind {
            Kind::Integer => {
                let canonical = self.integer(value)?.to_string();
                property.values.iter().position(|v| *v == canonical)
            }
            Kind::Boolean | Kind::Enum => property.values.iter().position(|v| v == value),
        }
    }
}
pub fn builtin() -> Result<&'static Table, &'static Error> {
    static TABLE: OnceLock<Result<Table, Error>> = OnceLock::new();
    TABLE
        .get_or_init(|| Table::load(include_bytes!("adventure.json")))
        .as_ref()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_tag_identifiers_use_native_defaults_without_asset_path_restrictions() {
        let facts = builtin().unwrap();
        assert_eq!(
            facts.identifier(":mineable/pickaxe").unwrap(),
            "minecraft:mineable/pickaxe"
        );
        assert_eq!(
            facts.identifier("mineable/pickaxe").unwrap(),
            "minecraft:mineable/pickaxe"
        );
        assert_eq!(
            facts.identifier("fixture:a//../b").unwrap(),
            "fixture:a//../b"
        );
        assert_eq!(facts.identifier("minecraft:").unwrap(), "minecraft:");
        assert!(facts.identifier("..:name").is_none());
        assert!(facts.identifier("UPPER:name").is_none());
    }
    #[test]
    fn property_matching_preserves_native_integer_parser_and_enum_comparable_order() {
        let facts = builtin().unwrap();
        let (_, block) = facts.block("minecraft:oak_stairs").unwrap();
        let facing = &block.properties["facing"];
        assert!(
            facts.property_index(facing, "north").unwrap()
                < facts.property_index(facing, "east").unwrap(),
            "enum order is not alphabetical string order"
        );
        let (_, block) = facts.block("minecraft:wheat").unwrap();
        let age = &block.properties["age"];
        assert_eq!(
            facts.property_index(age, "+01"),
            facts.property_index(age, "1")
        );
        assert_eq!(
            facts.property_index(age, "١"),
            facts.property_index(age, "1")
        );
        assert_eq!(
            facts.property_index(age, "𝟙"),
            None,
            "Integer.parseInt examines surrogate code units"
        );
        assert_eq!(facts.property_index(age, "2147483648"), None);
        assert_eq!(facts.property_index(age, " 1"), None);
        assert_eq!(
            facts.property_index(age, "8"),
            None,
            "parsed values must exist in the property domain"
        );
        assert!(facts.block("minecraft:chest").unwrap().1.block_entity);
        assert!(!facts.block("minecraft:stone").unwrap().1.block_entity);
    }
}
