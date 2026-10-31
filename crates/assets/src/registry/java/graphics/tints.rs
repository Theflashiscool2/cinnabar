//! Java 26.3 BlockColors / BlockTintSources bindings, independent of Bedrock block IDs.
//! Behavior reference and remaining callbacks: docs/references/java-block-colors.md.
use super::super::{BlockState, source::Error};
use crate::{
    MATERIAL_FLAG_BIRCH_FOLIAGE, MATERIAL_FLAG_DRY_FOLIAGE, MATERIAL_FLAG_EVERGREEN_FOLIAGE,
    MATERIAL_FLAG_FOLIAGE_TINT, MATERIAL_FLAG_GRASS_TINT, MATERIAL_FLAG_WATER_TINT,
};

/// Fixed Java foliage colors; callers populate these palette lanes in every biome.
pub const BIRCH: [u8; 3] = [128, 167, 85];
pub const SPRUCE: [u8; 3] = [97, 153, 97];

pub(super) fn material(state: &BlockState, index: i32) -> Result<u32, Error> {
    // A model's tint index selects a registered layer; an absent layer is unmodified,
    // including positive out-of-range indices. It does not imply a missing callback.
    if index < 0 {
        return Ok(0);
    }
    let name = &*state.name;
    if matches!(name, "minecraft:pink_petals" | "minecraft:wildflowers") {
        return Ok(if index == 1 {
            MATERIAL_FLAG_GRASS_TINT
        } else {
            0
        });
    }
    if index != 0 {
        return Ok(0);
    }
    Ok(match name {
        "minecraft:grass_block"
        | "minecraft:short_grass"
        | "minecraft:fern"
        | "minecraft:potted_fern"
        | "minecraft:bush"
        | "minecraft:sugar_cane" => MATERIAL_FLAG_GRASS_TINT,
        "minecraft:tall_grass" | "minecraft:large_fern" => MATERIAL_FLAG_GRASS_TINT,
        "minecraft:oak_leaves"
        | "minecraft:jungle_leaves"
        | "minecraft:acacia_leaves"
        | "minecraft:dark_oak_leaves"
        | "minecraft:vine"
        | "minecraft:mangrove_leaves" => MATERIAL_FLAG_FOLIAGE_TINT,
        "minecraft:birch_leaves" => MATERIAL_FLAG_FOLIAGE_TINT | MATERIAL_FLAG_BIRCH_FOLIAGE,
        "minecraft:spruce_leaves" => MATERIAL_FLAG_FOLIAGE_TINT | MATERIAL_FLAG_EVERGREEN_FOLIAGE,
        "minecraft:leaf_litter" => MATERIAL_FLAG_FOLIAGE_TINT | MATERIAL_FLAG_DRY_FOLIAGE,
        "minecraft:water_cauldron" | "minecraft:water" | "minecraft:bubble_column" => {
            MATERIAL_FLAG_WATER_TINT
        }
        "minecraft:attached_melon_stem" | "minecraft:attached_pumpkin_stem" => {
            crate::stem::material_flags(7).unwrap()
        }
        "minecraft:melon_stem" | "minecraft:pumpkin_stem" => state
            .properties
            .get("age")
            .and_then(|age| age.parse().ok())
            .and_then(crate::stem::material_flags)
            .ok_or_else(|| Error::Model(format!("invalid Java stem age for {name}")))?,
        _ => 0,
    })
}

/// Additional quad-local callbacks; absent layers must never modify texture colors.
pub(super) fn quad_flags(state: &BlockState, index: i32) -> Result<u32, Error> {
    if index != 0 {
        return Ok(0);
    }
    Ok(match &*state.name {
        "minecraft:redstone_wire" => {
            let power = state
                .properties
                .get("power")
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|v| *v <= 15)
                .ok_or_else(|| Error::Model("invalid Java redstone power".into()))?;
            crate::model::MODEL_QUAD_JAVA_TINT_REDSTONE
                | power << crate::model::MODEL_QUAD_JAVA_POWER_SHIFT
        }
        "minecraft:lily_pad" => crate::model::MODEL_QUAD_JAVA_TINT_LILY,
        "minecraft:tall_grass" | "minecraft:large_fern" => {
            match state.properties.get("half").map(String::as_str) {
                Some("upper") => crate::model::MODEL_QUAD_JAVA_TINT_BELOW,
                Some("lower") => 0,
                _ => return Err(Error::Model("invalid Java tall plant half".into())),
            }
        }
        _ => 0,
    })
}

/// Java BlockColors' in-world lily color (item/default color is deliberately different).
pub const LILY: [u8; 3] = [32, 128, 48];

/// RedStoneWireBlock's quantized gamma-space vertex color, before texture multiplication.
pub fn redstone(power: u8) -> Option<[u8; 3]> {
    if power > 15 {
        return None;
    }
    let f = f32::from(power) / 15.;
    Some(
        [
            f * 0.6 + if power > 0 { 0.4 } else { 0.3 },
            (f * f * 0.7 - 0.5).clamp(0., 1.),
            (f * f * 0.6 - 0.7).clamp(0., 1.),
        ]
        .map(|v| (v * 255.) as u8),
    )
}

/// Known intentionally empty models. This does not erase fluids inside invisible hosts.
pub(super) fn allows_empty_model(state: &BlockState) -> bool {
    match &*state.name {
        "minecraft:air"
        | "minecraft:cave_air"
        | "minecraft:void_air"
        | "minecraft:structure_void" => true,
        "minecraft:barrier" | "minecraft:light" => {
            state.properties.get("waterlogged").map(String::as_str) == Some("false")
        }
        "minecraft:pitcher_crop" => {
            state.properties.get("half").map(String::as_str) == Some("upper")
                && matches!(
                    state.properties.get("age").map(String::as_str),
                    Some("0" | "1" | "2")
                )
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests;
