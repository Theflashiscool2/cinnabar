//! Java block-state identity and properties from a hash-pinned official data-generator report.
pub mod banner;
pub mod catalog;
pub mod fluids;
pub mod geometry;
pub mod graphics;
pub mod interaction;
pub mod models;
pub mod physics;
pub mod selectors;
pub mod source;
pub mod special;
pub mod textures;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Java block report does not match the pinned server report SHA-256")]
    Hash,
    #[error("invalid Java block registry report: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid Java block registry report: {0}")]
    Invalid(&'static str),
}
#[derive(Clone, Debug)]
pub struct BlockState {
    pub name: Arc<str>,
    pub properties: BTreeMap<String, String>,
    pub is_default: bool,
}
#[derive(Debug)]
pub struct BlockRegistry {
    states: Box<[BlockState]>,
    digest: [u8; 32],
    block_ids: Arc<[u32]>,
}
#[derive(Deserialize)]
struct Block {
    #[serde(default)]
    properties: BTreeMap<String, Vec<String>>,
    states: Vec<State>,
}
#[derive(Deserialize)]
struct State {
    id: u32,
    #[serde(default)]
    properties: BTreeMap<String, String>,
    #[serde(default)]
    default: bool,
}
impl BlockRegistry {
    /// Verifies provenance before parsing; callers obtain the expected digest from the Java pin.
    pub fn load(bytes: &[u8], expected_sha256: &str) -> Result<Self, Error> {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        if format!("{:x}", Sha256::digest(bytes)) != expected_sha256 {
            return Err(Error::Hash);
        }
        let blocks: BTreeMap<String, Block> = serde_json::from_slice(bytes)?;
        let count = blocks.values().map(|b| b.states.len()).sum::<usize>();
        if count == 0 {
            return Err(Error::Invalid("empty report"));
        }
        let mut states = vec![None; count];
        for (name, block) in blocks {
            if !name.starts_with("minecraft:")
                || block.states.iter().filter(|s| s.default).count() != 1
            {
                return Err(Error::Invalid("block namespace/default state"));
            }
            let name: Arc<str> = name.into();
            for state in block.states {
                if state.properties.len() != block.properties.len()
                    || state.properties.iter().any(|(k, v)| {
                        !block
                            .properties
                            .get(k)
                            .is_some_and(|values| values.contains(v))
                    })
                {
                    return Err(Error::Invalid(
                        "state properties disagree with block definition",
                    ));
                }
                let slot = states
                    .get_mut(state.id as usize)
                    .ok_or(Error::Invalid("noncontiguous state ID"))?;
                if slot.is_some() {
                    return Err(Error::Invalid("duplicate state ID"));
                }
                *slot = Some(BlockState {
                    name: Arc::clone(&name),
                    properties: state.properties,
                    is_default: state.default,
                });
            }
        }
        let states: Box<[BlockState]> = states
            .into_iter()
            .map(|s| s.ok_or(Error::Invalid("missing state ID")))
            .collect::<Result<_, _>>()?;
        // Block.BLOCK_STATE_REGISTRY traverses BuiltInRegistries.BLOCK in registry order.
        // Each block contributes one contiguous state-definition run in this pinned report.
        let mut seen = std::collections::BTreeSet::new();
        let mut previous: Option<&str> = None;
        let mut block_id = 0;
        let mut block_ids = Vec::with_capacity(states.len());
        for state in &states {
            if previous != Some(&state.name) {
                if !seen.insert(state.name.as_ref()) {
                    return Err(Error::Invalid("non-contiguous block states"));
                }
                if previous.is_some() {
                    block_id += 1;
                }
                previous = Some(&state.name);
            }
            block_ids.push(block_id);
        }
        Ok(Self {
            states,
            digest,
            block_ids: block_ids.into(),
        })
    }
    pub fn state(&self, id: u32) -> Option<&BlockState> {
        self.states.get(id as usize)
    }
    pub fn block_types(&self) -> Arc<[u32]> {
        Arc::clone(&self.block_ids)
    }
    pub fn block_id(&self, state: u32) -> Option<u32> {
        self.block_ids.get(state as usize).copied()
    }
    pub fn len(&self) -> usize {
        self.states.len()
    }
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn load(s: &str) -> Result<BlockRegistry, Error> {
        BlockRegistry::load(s.as_bytes(), &format!("{:x}", Sha256::digest(s.as_bytes())))
    }
    #[test]
    fn properties_and_unknown_identity_stay_explicit() {
        let r=load(r#"{"minecraft:test":{"properties":{"facing":["north","south"]},"states":[{"id":1,"properties":{"facing":"south"}},{"id":0,"default":true,"properties":{"facing":"north"}}]}}"#).unwrap();
        assert_eq!(&*r.state(1).unwrap().name, "minecraft:test");
        assert_eq!(r.state(1).unwrap().properties["facing"], "south");
        assert!(r.state(2).is_none());
    }
    #[test]
    fn provenance_duplicate_ids_holes_and_bad_properties_are_rejected() {
        assert!(matches!(
            BlockRegistry::load(b"{}", "incorrect"),
            Err(Error::Hash)
        ));
        for s in [
            r#"{}"#,
            r#"{"minecraft:test":{"states":[{"id":1,"default":true}]}}"#,
            r#"{"minecraft:test":{"states":[{"id":0,"default":true},{"id":0}]}}"#,
            r#"{"minecraft:test":{"states":[{"id":0,"default":true,"properties":{"bad":"value"}}]}}"#,
        ] {
            assert!(load(s).is_err());
        }
    }
}
