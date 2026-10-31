//! Lazy, version-scoped Java block models. Call from asset/mesh workers, never from rendering.
use super::{
    BlockRegistry,
    geometry::{self, Quad},
    models::Models,
    selectors::{Selection, Variant},
    source::{Error, Identifier, Source},
};
use std::{collections::BTreeMap, sync::Arc};

pub struct BlockModels {
    registry: Arc<BlockRegistry>,
    source: Box<dyn Source + Send>,
    selections: BTreeMap<u32, Arc<Selection>>,
    models: Models,
    geometry: BTreeMap<(Identifier, [u16; 3], bool), Arc<[Quad]>>,
}
impl BlockModels {
    pub fn cancelled(&self) -> bool {
        self.source.cancelled()
    }
    pub fn new(registry: Arc<BlockRegistry>, source: Box<dyn Source + Send>) -> Self {
        Self {
            registry,
            source,
            selections: BTreeMap::new(),
            models: Models::default(),
            geometry: BTreeMap::new(),
        }
    }
    pub fn selection(&mut self, state: u32) -> Result<Arc<Selection>, Error> {
        if let Some(selection) = self.selections.get(&state) {
            return Ok(Arc::clone(selection));
        }
        let block = self
            .registry
            .state(state)
            .ok_or_else(|| Error::Model(format!("unknown Java block state {state}")))?;
        let id = Identifier::parse(&block.name)?;
        let value = self.source.json(&id.asset_path("blockstates", "json"))?;
        let selection = Arc::new(Selection::read(&value, block)?);
        self.selections.insert(state, Arc::clone(&selection));
        Ok(selection)
    }
    pub fn geometry(&mut self, variant: &Variant) -> Result<Arc<[Quad]>, Error> {
        let key = (variant.model.clone(), variant.rotations, variant.uv_lock);
        if let Some(geometry) = self.geometry.get(&key) {
            return Ok(Arc::clone(geometry));
        }
        let model = self.models.load(&mut *self.source, &variant.model)?;
        let geometry: Arc<[Quad]> = Arc::from(geometry::bake(&model, variant)?);
        self.geometry.insert(key, Arc::clone(&geometry));
        Ok(geometry)
    }
    /// Selects the actual weighted/multipart geometry for an authoritative world block.
    pub fn block(&mut self, state: u32, position: [i32; 3]) -> Result<Vec<Arc<[Quad]>>, Error> {
        let selection = self.selection(state)?;
        selection
            .choose(position)
            .into_iter()
            .map(|v| self.geometry(v))
            .collect()
    }
    pub fn state_name(&self, state: u32) -> Result<&str, Error> {
        self.registry
            .state(state)
            .map(|s| &*s.name)
            .ok_or_else(|| Error::Model(format!("unknown Java block state {state}")))
    }
    pub fn source(&mut self) -> &mut dyn Source {
        &mut *self.source
    }
}
