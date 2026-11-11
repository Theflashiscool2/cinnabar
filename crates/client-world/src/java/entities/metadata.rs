//! Admission of pinned metadata accessors into native appearance, independent of ECS.
use super::WorldState;
use java_protocol::{
    nbt::Value as Nbt,
    session::entities::metadata::{Packet, Value},
    target,
};
use std::sync::Arc;
use world::java::entities::visual::{
    MetadataGap, PigAppearance, PigModel, VillagerAppearance, Visual,
};

pub(super) fn default_villager() -> VillagerAppearance {
    VillagerAppearance {
        variant: target::VILLAGER_DEFAULT_TYPE.into(),
        profession: target::VILLAGER_DEFAULT_PROFESSION.into(),
        level: 1,
        unhappy_ticks: 0,
        finalized: false,
    }
}
fn villager_value(values: [i32; 3], current: &mut VillagerAppearance) -> Result<(), &'static str> {
    let lookup = |values: &'static [&'static str], id| {
        usize::try_from(id)
            .ok()
            .and_then(|id| values.get(id))
            .copied()
    };
    current.variant = lookup(target::VILLAGER_TYPES, values[0])
        .ok_or("unknown native villager type")?
        .into();
    current.profession = lookup(target::VILLAGER_PROFESSIONS, values[1])
        .ok_or("unknown native villager profession")?
        .into();
    // Pinned VillagerData constructor uses Math.max(1, level), with no upper clamp.
    current.level = values[2].max(1);
    Ok(())
}

pub(super) fn default_pig(state: &WorldState) -> Result<Arc<PigAppearance>, &'static str> {
    let entries = state
        .bootstrap
        .as_ref()
        .and_then(|b| b.configuration.registries.get("minecraft:pig_variant"))
        .ok_or("missing Java pig variant registry")?;
    // Vanilla VariantUtils.getDefaultOrAny prefers the target's default key, then registry order.
    let entry = entries
        .iter()
        .find(|e| e.name == target::PIG_DEFAULT_VARIANT)
        .or_else(|| entries.first())
        .ok_or("empty Java pig variant registry")?;
    pig(entry)
}
fn variant(state: &WorldState, id: i32) -> Result<Arc<PigAppearance>, &'static str> {
    let entries = state
        .bootstrap
        .as_ref()
        .and_then(|b| b.configuration.registries.get("minecraft:pig_variant"))
        .ok_or("missing Java pig variant registry")?;
    let entry = usize::try_from(id)
        .ok()
        .and_then(|i| entries.get(i))
        .ok_or("unknown Java pig variant")?;
    pig(entry)
}
fn pig(
    entry: &java_protocol::session::facts::RegistryEntry,
) -> Result<Arc<PigAppearance>, &'static str> {
    let Nbt::Compound(fields) = &entry.value else {
        return Err("Java pig variant compound");
    };
    let text = |key| match fields.get(key) {
        Some(Nbt::String(s)) => Ok(s.as_str()),
        _ => Err("Java pig texture identifier"),
    };
    let model = match fields.get("model") {
        None => PigModel::Normal,
        Some(Nbt::String(s)) if s == "normal" => PigModel::Normal,
        Some(Nbt::String(s)) if s == "cold" => PigModel::Cold,
        _ => return Err("unsupported Java pig model"),
    };
    let adult = text("asset_id")?;
    let baby = text("baby_asset_id")?;
    // Identifiers remain resource keys, never filesystem paths or interpreted server commands.
    for value in [adult, baby] {
        assets::registry::java::source::Identifier::parse(value)
            .map_err(|_| "invalid Java pig resource identifier")?;
    }
    Ok(Arc::new(PigAppearance {
        name: entry.name.as_str().into(),
        model,
        adult_texture: adult.into(),
        baby_texture: baby.into(),
    }))
}

pub(super) fn admit(
    state: &mut WorldState,
    scope: (u64, u64),
    own: i32,
    packet: Packet,
) -> Result<bool, &'static str> {
    if packet.id == own {
        // Local player metadata has its own authority; it is not a remote entity.
        let mut visual = state.player_visual.clone();
        apply(state, &mut visual, target::METADATA_PLAYER, packet)?;
        state.player_visual = visual;
        return Ok(true);
    }
    let Some(entity) = state.entities.get(packet.id) else {
        return state.entities.update(scope, packet.id, |_| Ok(()));
    };
    let Some(schema) = schema(&entity.kind) else {
        // Unknown schemas remain explicit; do not guess their inherited indexes.
        return state.entities.update(scope, packet.id, |e| {
            e.visual.incomplete = Some(MetadataGap {
                index: 0,
                serializer: "unimplemented entity metadata schema".into(),
            });
            Ok(())
        });
    };
    let use_receipt = packet.entries.iter().any(|entry| {
        schema
            .iter()
            .any(|spec| spec.index == entry.index && spec.name == "DATA_LIVING_ENTITY_FLAGS")
    });
    let id = packet.id;
    let identity = entity.identity;
    let left_handed = entity.visual.left_handed;
    let mut visual = entity.visual.clone();
    if let Some(item) = apply(state, &mut visual, schema, packet)? {
        state.equipment.set_ground(identity, item)?;
    }
    if use_receipt {
        super::super::equipment::observe_remote_use(&state.equipment, identity, &mut visual);
    }
    if left_handed != visual.left_handed {
        state.equipment.appearance_changed(identity);
    }
    state.entities.update(scope, id, |e| {
        e.visual = visual;
        Ok(())
    })
}
fn apply(
    state: &WorldState,
    visual: &mut Visual,
    schema: &[target::MetadataSpec],
    packet: Packet,
) -> Result<Option<inventory::java::Slot>, &'static str> {
    let mut item = None;
    if let Some(gap) = packet.unsupported {
        visual.incomplete = Some(MetadataGap {
            index: gap.index,
            serializer: gap.serializer.into(),
        });
        return Ok(None);
    }
    for entry in packet.entries {
        let Some(spec) = schema.iter().find(|spec| spec.index == entry.index) else {
            return Err("unknown Java entity metadata accessor");
        };
        if spec.serializer != entry.serializer {
            return Err("Java entity metadata serializer mismatch");
        }
        match (spec.name, entry.value) {
            ("DATA_ITEM", Value::Item(stack)) => {
                item = Some(super::super::inventory::stack(stack)?)
            }
            ("DATA_SHARED_FLAGS_ID", Value::Byte(flags)) => visual.flags = flags,
            ("DATA_NO_GRAVITY", Value::Bool(value)) => visual.no_gravity = value,
            ("DATA_CUSTOM_NAME", Value::Component(name)) => {
                visual.custom_name = name.map(super::super::nbt::convert).map(Arc::new)
            }
            ("DATA_CUSTOM_NAME_VISIBLE", Value::Bool(visible)) => {
                visual.custom_name_visible = visible
            }
            ("DATA_CLIENT_FLAGS", Value::Byte(flags)) if visual.armor_stand_flags.is_some() => {
                visual.armor_stand_flags = Some(flags)
            }
            ("DATA_LIVING_ENTITY_FLAGS", Value::Byte(flags)) => visual.observe_use(flags)?,
            ("DATA_POSE", Value::Int(pose)) => {
                visual.pose = usize::try_from(pose)
                    .ok()
                    .and_then(|i| target::ENTITY_POSES.get(i))
                    .ok_or("invalid Java entity pose")?
                    .to_owned()
                    .into();
            }
            ("DATA_BABY_ID", Value::Bool(baby)) => visual.baby = baby,
            ("DATA_HEALTH_ID", Value::Float(health)) => visual.health = Some(health),
            ("DATA_PLAYER_MAIN_HAND", Value::Int(hand)) => {
                visual.left_handed = match hand {
                    0 => true,
                    1 => false,
                    _ => return Err("Invalid native humanoid arm"),
                };
            }
            ("DATA_PLAYER_MODE_CUSTOMISATION", Value::Byte(parts)) => {
                visual.player_layers = world::java::entities::visual::PlayerLayers {
                    hat: parts & target::PLAYER_MODEL_PART_HAT != 0,
                    jacket: parts & target::PLAYER_MODEL_PART_JACKET != 0,
                    left_sleeve: parts & target::PLAYER_MODEL_PART_LEFT_SLEEVE != 0,
                    right_sleeve: parts & target::PLAYER_MODEL_PART_RIGHT_SLEEVE != 0,
                    left_pants: parts & target::PLAYER_MODEL_PART_LEFT_PANTS_LEG != 0,
                    right_pants: parts & target::PLAYER_MODEL_PART_RIGHT_PANTS_LEG != 0,
                    cape: parts & target::PLAYER_MODEL_PART_CAPE != 0,
                };
            }
            ("DATA_VILLAGER_DATA", Value::Villager(values)) => {
                villager_value(
                    values,
                    visual
                        .villager
                        .as_mut()
                        .ok_or("villager data on another native entity")?,
                )?;
            }
            ("DATA_UNHAPPY_COUNTER", Value::Int(ticks)) if visual.villager.is_some() => {
                visual.villager.as_mut().unwrap().unhappy_ticks = ticks;
            }
            ("DATA_VILLAGER_DATA_FINALIZED", Value::Bool(finalized))
                if visual.villager.is_some() =>
            {
                visual.villager.as_mut().unwrap().finalized = finalized;
            }
            ("DATA_VARIANT_ID", Value::Int(id)) if visual.pig.is_some() => {
                visual.pig = Some(variant(state, id)?)
            }
            // Correctly framed, schema-checked facts with no appearance consumer in this slice.
            _ => {}
        }
    }
    Ok(item)
}

pub(super) fn appearances(state: &WorldState) -> Result<Vec<Arc<PigAppearance>>, &'static str> {
    state
        .bootstrap
        .as_ref()
        .and_then(|b| b.configuration.registries.get("minecraft:pig_variant"))
        .ok_or("missing Java pig variant registry")?
        .iter()
        .map(pig)
        .collect()
}

fn schema(kind: &str) -> Option<&'static [target::MetadataSpec]> {
    Some(match kind {
        target::ENTITY_PIG => target::METADATA_PIG,
        target::ENTITY_PLAYER => target::METADATA_PLAYER,
        target::ENTITY_ITEM => target::METADATA_ITEM,
        target::ENTITY_VILLAGER => target::METADATA_VILLAGER,
        target::ENTITY_ARMOR_STAND => target::METADATA_ARMOR_STAND,
        _ => return None,
    })
}
/// Only identified native living schemas may consume living animation/effect facts.
pub(in crate::java) fn living(kind: &str) -> bool {
    schema(kind).is_some_and(|schema| {
        schema
            .iter()
            .any(|spec| spec.name == "DATA_LIVING_ENTITY_FLAGS")
    })
}
