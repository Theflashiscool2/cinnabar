//! Native player armor resource selection. Geometry comes from verified runtime
//! model factories; textures remain resource data, never local file instructions.
use super::{
    items::{Stack, defaults},
    source::{Identifier, Source},
    textures::Image,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone)]
pub struct Armor {
    pub model: String,
    pub layers: Vec<Image>,
}
#[derive(Deserialize)]
struct Definition {
    layers: BTreeMap<String, Vec<Layer>>,
}
#[derive(Deserialize)]
struct Layer {
    texture: String,
    #[serde(default)]
    dyeable: Option<Dyeable>,
}
#[derive(Deserialize)]
struct Dyeable {
    #[serde(default)]
    color_when_undyed: Option<i32>,
}
pub fn component<'a>(stack: &'a Stack, key: &str) -> Option<&'a Value> {
    if stack.removed.contains(key) {
        None
    } else {
        stack
            .added
            .get(key)
            .or_else(|| defaults::component(&stack.item, key))
    }
}
fn slot(value: &Value) -> Option<u8> {
    value
        .as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| *v < 8)
        .or_else(|| {
            [
                "mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle",
            ]
            .iter()
            .position(|s| Some(*s) == value.as_str())
            .map(|v| v as u8)
        })
}
/// Returns None for a legitimate non-armor equippable. Its head/wing/saddle layer
/// is a distinct renderer; callers must not substitute a humanoid armor mesh.
pub fn armor(
    source: &mut dyn Source,
    stack: &Stack,
    worn_slot: u8,
    slim: bool,
) -> Result<Option<Armor>, String> {
    let Some(eq) = component(stack, "minecraft:equippable") else {
        return Ok(None);
    };
    if eq.get("slot").and_then(slot) != Some(worn_slot) {
        return Ok(None);
    }
    let Some(asset) = eq
        .get("asset_id")
        .or_else(|| eq.get("model"))
        .filter(|v| !v.is_null())
    else {
        return Ok(None);
    };
    let asset = Identifier::parse(asset.as_str().ok_or("invalid native equipment asset")?)
        .map_err(|e| e.to_string())?;
    let name = match worn_slot {
        2 => "boots",
        3 => "leggings",
        4 => "chestplate",
        5 => "helmet",
        _ => return Ok(None),
    };
    let layer_type = if worn_slot == 3 {
        "humanoid_leggings"
    } else {
        "humanoid"
    };
    let bytes = source
        .read(&asset.asset_path("equipment", "json"))
        .map_err(|e| e.to_string())?;
    let definition: Definition = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let Some(layers) = definition.layers.get(layer_type) else {
        return Ok(None);
    };
    if layers.len() > 24 {
        return Err("native equipment layer budget exhausted".into());
    }
    let mut images = Vec::with_capacity(layers.len());
    for layer in layers {
        if source.cancelled() {
            return Err("native equipment resource preparation cancelled".into());
        }
        let texture = Identifier::parse(&layer.texture).map_err(|e| e.to_string())?;
        let path = format!(
            "assets/{}/textures/entity/equipment/{layer_type}/{}.png",
            texture.namespace(),
            texture.path()
        );
        let mut image = Image::decode(&source.read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if let Some(dye) = &layer.dyeable {
            let color = component(stack, "minecraft:dyed_color")
                .map(|v| {
                    v.as_i64()
                        .and_then(|v| i32::try_from(v).ok())
                        .ok_or("invalid native dyed color")
                })
                .transpose()?
                .or(dye.color_when_undyed);
            // A dyeable layer without either native color is not drawn.
            let Some(color) = color else { continue };
            let color = color as u32;
            for pixel in image.rgba.chunks_exact_mut(4) {
                for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
                    pixel[channel] = (f32::from(pixel[channel]) * ((color >> shift) & 255) as f32
                        / 255.)
                        .round() as u8;
                }
            }
        }
        images.push(image);
    }
    Ok(Some(Armor {
        model: format!(
            "armor/{}/{name}",
            if slim { "player_slim" } else { "player" }
        ),
        layers: images,
    }))
}

#[cfg(test)]
mod tests;
