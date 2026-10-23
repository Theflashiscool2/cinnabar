//! Java terrain authority. Packet schemas stay in java-protocol; render consumers use these facts.
pub mod dynamics;
pub mod entities;
pub mod hud;
mod light;
mod palette;
pub mod player;
mod store;
pub use store::{PublicationKey, Terrain};
#[cfg(test)]
mod tests;
use crate::NbtValue;
pub use palette::{BiomeId, BlockStateId, Palette};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dimension {
    pub session: u64,
    pub epoch: u64,
    pub name: String,
    pub min_y: i32,
    pub height: i32,
    pub has_skylight: bool,
}
#[derive(Clone, Debug)]
pub struct Section {
    pub blocks: Palette<BlockStateId>,
    pub biomes: Palette<BiomeId>,
}
/// Native light storage: absent layers are unallocated; present empty slices are zero.
/// In an incremental update, an absent layer instead means to retain existing storage.
#[derive(Clone, Debug, Default)]
pub struct Light {
    pub sky: Vec<Option<Arc<[u8]>>>,
    pub block: Vec<Option<Arc<[u8]>>>,
}
#[derive(Clone, Debug)]
pub struct BlockEntity {
    pub position: [i32; 3],
    pub kind: u32,
    pub data: Option<NbtValue>,
}
#[derive(Clone, Debug)]
pub struct Column {
    pub sections: Vec<Arc<Section>>,
    pub light: Light,
    pub heightmaps: Vec<(u32, Vec<u64>)>,
    pub block_entities: Vec<BlockEntity>,
    revision: u64,
}
impl Column {
    pub fn new(
        sections: Vec<Arc<Section>>,
        light: Light,
        heightmaps: Vec<(u32, Vec<u64>)>,
        block_entities: Vec<BlockEntity>,
    ) -> Self {
        Self {
            sections,
            light,
            heightmaps,
            block_entities,
            revision: 0,
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// Native biome appearance received in Java's configuration registry. IDs are list positions.
#[derive(Clone, Debug)]
pub struct BiomeAppearance {
    pub name: String,
    pub temperature: f32,
    pub downfall: f32,
    pub grass_color: Option<[u8; 3]>,
    pub foliage_color: Option<[u8; 3]>,
    pub water_color: [u8; 3],
    pub grass_modifier: Option<String>,
}
