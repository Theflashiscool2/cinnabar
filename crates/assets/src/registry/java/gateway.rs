//! Pinned Java End-gateway artwork and state identity.
use super::{
    BlockState,
    source::{Error, Source},
    textures::Image,
};
use std::{collections::BTreeMap, sync::Arc};
pub const BEAM: &str = "textures/entity/end_portal/end_gateway_beam";
pub const LAYERS: usize = 16;
/// Native DyeColor texture-diffuse values, independently measured by InspectGateway.
pub const SPAWN_COLOR: u32 = 0xc74ebd;
pub const COOLDOWN_COLOR: u32 = 0x8932b8;
pub fn matches(state: &BlockState) -> bool {
    state.name.as_ref() == "minecraft:end_gateway"
}
pub fn load(
    source: &mut dyn Source,
    images: &mut BTreeMap<String, Arc<Image>>,
) -> Result<(), Error> {
    images.insert(
        BEAM.into(),
        Arc::new(Image::decode(
            &source.read(&format!("assets/minecraft/{BEAM}.png"))?,
        )?),
    );
    Ok(())
}
