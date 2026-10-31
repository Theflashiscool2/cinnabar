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
        "minecraft:tall_grass" | "minecraft:large_fern" => {
            if state.properties.get("half").map(String::as_str) != Some("lower") {
                return Err(Error::Model(format!(
                    "Java block tint callback for {name}: upper-half sampling at the block below is not implemented"
                )));
            }
            MATERIAL_FLAG_GRASS_TINT
        }
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
        "minecraft:redstone_wire" | "minecraft:lily_pad" => {
            return Err(Error::Model(format!(
                "Java block tint callback for {name} index {index} is not implemented"
            )));
        }
        _ => 0,
    })
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
