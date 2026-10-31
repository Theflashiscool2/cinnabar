//! Supported component wire values. Unsupported required item data fails with its identifier.
use super::*;
use serde_json::json;
#[cfg(test)]
mod tests;

pub(super) fn read(spec: &target::ItemComponent, r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(match spec.reader {
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
        _ => return Err(Error::UnsupportedItemComponent(spec.name)),
    })
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
