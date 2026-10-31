//! Version-matched End portal artwork and palette; never execute resource-pack shaders.
use super::{
    source::{Error, Source},
    textures::Image,
};
use std::{collections::BTreeMap, sync::Arc};
pub fn matches(state: &super::BlockState) -> bool {
    state.name.as_ref() == "minecraft:end_portal"
}
pub const SKY: &str = "textures/environment/end_sky";
pub const STARS: &str = "textures/entity/end_portal/end_portal";
/// RenderPipelines.END_PORTAL in the pinned client (gateway uses a separate count).
pub const LAYERS: usize = 15;
#[derive(Clone, Debug)]
pub struct Resources {
    pub colors: [[f32; 4]; 16],
}

pub fn load(
    source: &mut dyn Source,
    images: &mut BTreeMap<String, Arc<Image>>,
) -> Result<Resources, Error> {
    let bytes = source.read("assets/minecraft/shaders/core/rendertype_end_portal.fsh")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
    let colors = palette(text)?;
    for name in [SKY, STARS] {
        images.insert(
            name.into(),
            Arc::new(Image::decode(
                &source.read(&format!("assets/minecraft/{name}.png"))?,
            )?),
        );
    }
    Ok(Resources { colors })
}
fn invalid() -> Error {
    Error::Model("Java End portal palette is missing or unsupported".into())
}
/// Read only the bounded numeric palette declaration, not arbitrary shader expressions.
fn palette(text: &str) -> Result<[[f32; 4]; 16], Error> {
    if text.len() > 64 * 1024 {
        return Err(invalid());
    }
    let body = text
        .split_once("COLORS = vec3[](")
        .ok_or_else(invalid)?
        .1
        .split_once(");")
        .ok_or_else(invalid)?
        .0;
    let mut rest = body.trim();
    let mut colors = [[0.; 4]; 16];
    for color in &mut colors {
        let tail = rest.strip_prefix("vec3(").ok_or_else(invalid)?;
        let (values, tail) = tail.split_once(')').ok_or_else(invalid)?;
        let components = values.split(',').map(str::trim).collect::<Vec<_>>();
        if components.len() != 3 {
            return Err(invalid());
        }
        for (out, value) in color[..3].iter_mut().zip(components) {
            *out = value.parse::<f32>().map_err(|_| invalid())?;
            if !out.is_finite() || !(0. ..=1.).contains(out) {
                return Err(invalid());
            }
        }
        rest = tail.trim().strip_prefix(',').unwrap_or(tail.trim()).trim();
    }
    if !rest.is_empty() {
        return Err(invalid());
    }
    Ok(colors)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_palette_requires_exact_finite_triples() {
        let source = format!(
            "COLORS = vec3[]( {} );",
            vec!["vec3(0.125, 0.25, 0.5)"; 16].join(",")
        );
        assert_eq!(palette(&source).unwrap(), [[0.125, 0.25, 0.5, 0.]; 16]);
        for bad in [
            source.replace("0.125", "NaN"),
            source.replace("0.125", "1.1"),
            source.replace("0.125", "sin(1)"),
            source.replacen("vec3(0.125, 0.25, 0.5),", "", 1),
        ] {
            assert!(palette(&bad).is_err());
        }
    }
}
