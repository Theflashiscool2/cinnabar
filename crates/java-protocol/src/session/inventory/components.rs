//! Supported component wire values. Unsupported required item data fails with its identifier.
use super::*;
use serde_json::json;

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
