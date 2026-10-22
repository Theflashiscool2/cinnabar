//! Obtains terrain decoding context from the server's actual configuration registry.
use super::*;
impl Connection {
    pub(super) fn prepare_terrain(&mut self) -> Result<(), Error> {
        let dimensions = self
            .configuration
            .registries
            .get("minecraft:dimension_type")
            .ok_or(ProtocolError::Invalid("Java dimension registry"))?;
        let biomes = self
            .configuration
            .registries
            .get("minecraft:worldgen/biome")
            .ok_or(ProtocolError::Invalid("missing Java biome registry"))?
            .len();
        self.dimensions = dimensions
            .iter()
            .map(|entry| {
                let crate::nbt::Value::Compound(fields) = &entry.value else {
                    return Err(ProtocolError::Invalid("Java dimension registry compound"));
                };
                let int = |name| match fields.get(name) {
                    Some(crate::nbt::Value::Int(n)) => Ok(*n),
                    _ => Err(ProtocolError::Invalid("missing Java dimension integer")),
                };
                crate::terrain::Context::new(int("min_y")?, int("height")?, biomes)
            })
            .collect::<Result<_, _>>()?;
        Ok(())
    }
    pub(super) fn terrain_context(
        &self,
        spawn: &facts::Spawn,
    ) -> Result<crate::terrain::Context, Error> {
        usize::try_from(spawn.dimension_type)
            .ok()
            .and_then(|id| self.dimensions.get(id))
            .copied()
            .ok_or_else(|| ProtocolError::Invalid("Java dimension registry ID").into())
    }
}
