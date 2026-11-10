//! One bounded atlas/rig writer for every native world-item presentation context.
use super::*;
pub(super) struct Writer<'a> {
    pub output: &'a mut Output,
    pub rasters: &'a mut Vec<EquipmentRaster>,
    pub indices: &'a mut Vec<(Identity, usize)>,
    pub pixels: &'a mut usize,
    pub vertices: &'a mut usize,
}
impl Writer<'_> {
    pub fn append(
        &mut self,
        id: Identity,
        draws: &mut Vec<Draw>,
        hand: &java_item::Prepared,
        attachment: Option<bool>,
        glint: bool,
    ) -> Result<(), String> {
        let bytes = usize::from(hand.atlas.width) * usize::from(hand.atlas.height) * 4;
        for (i, geometry) in hand.groups.iter().enumerate() {
            charge(self.pixels, bytes, render_model::MAX_ACTOR_GPU_PIXEL_BYTES)?;
            charge(
                self.vertices,
                geometry.vertices.len() * std::mem::size_of::<render_model::ActorRigVertex>(),
                render_model::MAX_ACTOR_CATALOG_VERTEX_BYTES,
            )?;
            let mut geometry = geometry.clone();
            geometry.id = java_equipment_rig_id(
                65536 + u32::try_from(self.output.geometries.len()).map_err(|e| e.to_string())?,
            )
            .ok_or("native equipment rig budget exhausted")?;
            let rig = geometry.id;
            self.output.geometries.push(geometry);
            self.rasters.push(EquipmentRaster {
                width: hand.atlas.width,
                height: hand.atlas.height,
                rgba8: hand.atlas.rgba8.range(i * bytes, bytes).into_owned().into(),
            });
            self.indices.push((id, draws.len()));
            draws.push(Draw {
                source: None,
                model: None,
                rig,
                hand: attachment,
                bounds: hand.bounds,
                artwork: placeholder(),
                alpha: hand.alpha_modes[i],
                glint,
            });
        }
        Ok(())
    }
}
