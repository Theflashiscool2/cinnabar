//! Supported component wire values. Unsupported required item data fails with its identifier.
use super::*;
use serde_json::json;
mod common;
mod equipment;
mod presentation;
mod use_data;
use common::*;
#[cfg(test)]
mod tests;

pub(super) fn read(
    spec: &target::ItemComponent,
    r: &mut Reader<'_>,
    depth: usize,
) -> Result<Value, Error> {
    check_depth(depth)?;
    let value = match spec.reader {
        "MinecraftTypes::readVarInt" => json!(r.var()?),
        "ByteBuf::readFloat" => json!(r.f32()?),
        "ByteBuf::readInt" => json!(r.i32()?),
        "ByteBuf::readBoolean" => json!(r.bool()?),
        "unitReader()" => Value::Null,
        "MinecraftTypes::readResourceLocation" => json!(r.string(32767)?),
        "MinecraftTypes::readComponent" | "MinecraftTypes::readCompoundTagOrThrow" => {
            let value = nbt::read(r)?;
            if spec.reader == "MinecraftTypes::readCompoundTagOrThrow"
                && !matches!(value, nbt::Value::Compound(_))
            {
                return Err(Error::Invalid("item component expected compound NBT"));
            }
            serde_json::to_value(value)?
        }
        "ItemTypes::readItemEnchantments" => {
            let mut entries = BTreeMap::new();
            for _ in 0..r.count(r.0.len() / 2)? {
                let id = nonnegative(r)?;
                if entries.insert(id, nonnegative(r)?).is_some() {
                    return Err(Error::Invalid("duplicate native enchantment"));
                }
            }
            json!(entries)
        }
        "MinecraftTypes::readHolderSet" => holder_set(r)?,
        "ItemTypes::readUseEffects" => {
            json!({"can_sprint":r.bool()?,"interact_vibrations":r.bool()?,"speed_multiplier":r.f32()?})
        }
        "ItemTypes::readFoodProperties" => {
            json!({"nutrition":nonnegative(r)?,"saturation_modifier":r.f32()?,"can_always_eat":r.bool()?})
        }
        "ItemTypes::readWeapon" => {
            json!({"item_damage_per_attack":nonnegative(r)?,"disable_blocking_for_seconds":r.f32()?})
        }
        "ItemTypes::readToolData" => tool(r)?,
        "ItemTypes::readItemAttributeModifiers" => attribute_modifiers(r)?,
        "emptyNbtReader()" => {
            if r.0.first() == Some(&0) {
                r.byte()?;
            } else {
                nbt::read(r)?;
            }
            Value::Null
        }
        "ItemTypes::readCustomModelData" => presentation::custom_model(r)?,
        "MinecraftTypes::readResolvableProfile" => presentation::profile(r)?,
        "ItemTypes::readWritableBookContent" => presentation::book(r, false)?,
        "ItemTypes::readWrittenBookContent" => presentation::book(r, true)?,
        "ItemTypes::readSignText" => presentation::sign(r)?,
        "ItemTypes::readFireworkExplosion" => presentation::explosion(r)?,
        "ItemTypes::readFireworks" => {
            json!({"flight_duration":r.var()?,"explosions":list(r, 5, presentation::explosion)?})
        }
        "ItemTypes::readBlockStateProperties" => presentation::properties(r)?,
        "ItemTypes::readRecipes" => {
            let value = nbt::read(r)?;
            if !matches!(value, nbt::Value::List(_)) {
                return Err(Error::Invalid("item recipes expected list NBT"));
            }
            serde_json::to_value(value)?
        }
        "ItemTypes::readLodestoneTarget" => {
            json!({"target":optional(r, |r| Ok(json!({"dimension":key(r)?,"position":r.i64()?})))?,"tracked":r.bool()?})
        }
        "MinecraftTypes::readSound" => sound(r)?,
        "MinecraftTypes::readResolvableInt" => resolvable(r, false)?,
        "ItemTypes::readCookingFuel" => {
            json!({"burn_time":resolvable(r,false)?,"speed_multiplier":resolvable(r,true)?})
        }
        "ItemTypes::readBrewingFuel" => {
            json!({"uses":resolvable(r,false)?,"speed_multiplier":resolvable(r,true)?})
        }
        "ItemTypes::readMobVisibility" => json!({"entities":holder_set(r)?,"multiplier":r.f32()?}),
        "MinecraftTypes::readPaintingVariant" => holder(r, presentation::painting)?,
        "ItemTypes::readArmorTrim" => {
            json!({"material":holder(r,presentation::trim_material)?,"pattern":holder(r,presentation::trim_pattern)?})
        }
        "MinecraftTypes::readItemStackTemplate" => template(r, depth + 1)?,
        "ItemTypes::readPotDecorations" => {
            let mut sides = Vec::new();
            for _ in 0..4 {
                sides.push(optional(r, |r| template(r, depth + 1))?);
            }
            json!({"sides":sides})
        }
        "ItemTypes::readAdventureModePredicate" => equipment::predicates(r, depth + 1)?,
        "ItemTypes::readAttackRange" => equipment::attack_range(r)?,
        "ItemTypes::readEquippable" => equipment::equippable(r)?,
        "ItemTypes::readBlocksAttacks" => equipment::blocks_attacks(r)?,
        "ItemTypes::readPiercingWeapon" => equipment::piercing(r)?,
        "ItemTypes::readKineticWeapon" => equipment::kinetic(r)?,
        "ItemTypes::readSwingAnimation" => json!({"type":nonnegative(r)?,"duration":r.var()?}),
        "ItemTypes::readUseCooldown" => json!({"seconds":r.f32()?,"group":optional(r,key)?}),
        "ItemTypes::readConsumable" => use_data::consumable(r, depth + 1)?,
        "ItemTypes::readPotionContents" => use_data::potion(r, depth + 1)?,
        "ItemTypes::readTooltipDisplay" => {
            let hide = r.bool()?;
            let mut hidden = Vec::new();
            for _ in 0..r.count(target::ITEM_COMPONENTS.len())? {
                hidden.push(component(r)?.name);
            }
            json!({"hide_tooltip":hide,"hidden_components":hidden})
        }
        _ if spec.name == "minecraft:lore" => {
            let mut values = Vec::new();
            for _ in 0..r.count(256)? {
                values.push(nbt::read(r)?);
            }
            serde_json::to_value(values)?
        }
        _ => match spec.name {
            "minecraft:charged_projectiles" | "minecraft:bundle_contents" => {
                let max = if spec.name == "minecraft:charged_projectiles" {
                    1024
                } else {
                    usize::MAX
                };
                list_limit(r, 4, max, |r| template(r, depth + 1))?
            }
            "minecraft:container" => list(r, 1, |r| optional(r, |r| template(r, depth + 1)))?,
            "minecraft:death_protection" => list(r, 1, |r| use_data::consume_effect(r, depth + 1))?,
            "minecraft:suspicious_stew_effects" => list(r, 2, |r| {
                Ok(json!({"effect":nonnegative(r)?,"duration":r.var()?}))
            })?,
            "minecraft:banner_patterns" => list(r, 2, |r| {
                Ok(
                    json!({"pattern":holder(r,|r| Ok(json!({"asset":key(r)?,"translation":text(r)?})))?,"color":nonnegative(r)?}),
                )
            })?,
            "minecraft:entity_data" | "minecraft:block_entity_data" => typed_entity(r)?,
            "minecraft:instrument" => holder(r, presentation::instrument)?,
            "minecraft:provides_trim_material" => holder(r, presentation::trim_material)?,
            "minecraft:jukebox_playable" => holder(r, presentation::song)?,
            "minecraft:bees" => list(r, 4, |r| {
                Ok(
                    json!({"entity":typed_entity(r)?,"ticks_in_hive":r.var()?,"min_ticks_in_hive":r.var()?}),
                )
            })?,
            _ => return Err(Error::UnsupportedItemComponent(spec.name)),
        },
    };
    retained_bytes(&value)?;
    Ok(value)
}

/// Conservative retained JSON budget, separate from the encoded packet's byte limit.
pub(super) fn retained_bytes(value: &Value) -> Result<usize, Error> {
    fn visit(value: &Value, used: &mut usize) -> Result<(), Error> {
        *used = used.saturating_add(128);
        match value {
            Value::String(s) => *used = used.saturating_add(s.capacity()),
            Value::Array(values) => {
                for value in values {
                    visit(value, used)?;
                }
            }
            Value::Object(values) => {
                for (key, value) in values {
                    *used = used.saturating_add(key.capacity());
                    visit(value, used)?;
                }
            }
            _ => {}
        }
        if *used > crate::stream::MAX_UNCOMPRESSED {
            return Err(Error::Invalid("expanded item component budget"));
        }
        Ok(())
    }
    let mut used = 0;
    visit(value, &mut used)?;
    Ok(used)
}

fn attribute_modifiers(r: &mut Reader<'_>) -> Result<Value, Error> {
    let mut modifiers = Vec::new();
    // Minimum entry: attribute, key length, double, operation, slot and display.
    for _ in 0..r.count(r.0.len() / 13)? {
        let attribute = target::ATTRIBUTES
            .get(nonnegative(r)? as usize)
            .ok_or(Error::Invalid("unknown item modifier attribute"))?;
        let id = super::super::dynamics::identifier(r.string(32767)?)?;
        let amount = r.f64()?;
        // Item operations are VarInts, unlike the entity-attribute modifier byte.
        let operation = target::ATTRIBUTE_OPERATIONS
            .get(nonnegative(r)? as usize)
            .ok_or(Error::Invalid("item attribute modifier operation"))?;
        let slot = target::ITEM_ATTRIBUTE_SLOTS
            .get(nonnegative(r)? as usize)
            .ok_or(Error::Invalid("item attribute modifier slot"))?;
        let display_id = r.var()?;
        // The pinned DisplayType codec maps unknown IDs to DEFAULT with no extra payload.
        let display = usize::try_from(display_id)
            .ok()
            .and_then(|id| target::ITEM_ATTRIBUTE_DISPLAYS.get(id))
            .unwrap_or(&target::ITEM_ATTRIBUTE_DISPLAYS[0]);
        let component = if *display == "OVERRIDE" {
            Some(nbt::read(r)?)
        } else {
            None
        };
        modifiers.push(json!({
            "attribute": attribute.name,
            "modifier": {"id":id,"amount":amount,"operation":operation.to_ascii_lowercase()},
            "slot":slot.to_ascii_lowercase(),
            "display":{"type":display.to_ascii_lowercase(),"component":component}
        }));
    }
    Ok(json!({"modifiers":modifiers}))
}
fn holder_set(r: &mut Reader<'_>) -> Result<Value, Error> {
    let count = nonnegative(r)?;
    if count == 0 {
        return Ok(json!({"tag":r.string(32767)?}));
    }
    let count = (count - 1) as usize;
    if count > r.0.len() {
        return Err(Error::Truncated);
    }
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        ids.push(nonnegative(r)?);
    }
    Ok(json!({"ids":ids}))
}
fn tool(r: &mut Reader<'_>) -> Result<Value, Error> {
    let mut rules = Vec::new();
    for _ in 0..r.count(r.0.len())? {
        let blocks = holder_set(r)?;
        let speed = if r.bool()? { Some(r.f32()?) } else { None };
        let correct = if r.bool()? { Some(r.bool()?) } else { None };
        rules.push(json!({"blocks":blocks,"speed":speed,"correct_for_drops":correct}));
    }
    Ok(
        json!({"rules":rules,"default_mining_speed":r.f32()?,"damage_per_block":nonnegative(r)?,"can_destroy_blocks_in_creative":r.bool()?}),
    )
}
