//! Single-world immutable moving-block extraction; texture identity fences late uploads.
use bevy::{prelude::Resource, render::extract_resource::ExtractResource};
use std::sync::Arc;
#[derive(Clone, Default, Resource, ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
pub struct PosedBlocks {
    pub identity: Option<crate::ChunkTextureBindingIdentity>,
    /// Cells replaced by the posed geometry until their authoritative static mesh is resident.
    pub replaced: Arc<[[i32; 3]]>,
    pub solid: Arc<[render_model::posed_block::Vertex]>,
    pub blend: Arc<[render_model::posed_block::Vertex]>,
}
