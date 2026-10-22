//! Admission of pinned metadata accessors into native appearance, independent of ECS.
use super::WorldState;
use java_protocol::{
    nbt::Value as Nbt,
    session::entities::metadata::{Packet, Value},
    target,
};
use std::sync::Arc;
use world::java::entities::visual::{MetadataGap, PigAppearance, PigModel, Visual};

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
    let schema = match entity.kind.as_ref() {
        target::ENTITY_PIG => target::METADATA_PIG,
        target::ENTITY_PLAYER => target::METADATA_PLAYER,
        target::ENTITY_ITEM => target::METADATA_ITEM,
        // Other entity schemas have not been admitted; do not guess their inherited indexes.
        _ => {
            return state.entities.update(scope, packet.id, |e| {
                e.visual.incomplete = Some(MetadataGap {
                    index: 0,
                    serializer: "unimplemented entity metadata schema".into(),
                });
                Ok(())
            });
        }
    };
    let id = packet.id;
    let mut visual = entity.visual.clone();
    apply(state, &mut visual, schema, packet)?;
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
) -> Result<(), &'static str> {
    if let Some(gap) = packet.unsupported {
        visual.incomplete = Some(MetadataGap {
            index: gap.index,
            serializer: gap.serializer.into(),
        });
        return Ok(());
    }
    for entry in packet.entries {
        let Some(spec) = schema.iter().find(|spec| spec.index == entry.index) else {
            return Err("unknown Java entity metadata accessor");
        };
        if spec.serializer != entry.serializer {
            return Err("Java entity metadata serializer mismatch");
        }
        match (spec.name, entry.value) {
            ("DATA_SHARED_FLAGS_ID", Value::Byte(flags)) => visual.flags = flags,
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
            ("DATA_VARIANT_ID", Value::Int(id)) if visual.pig.is_some() => {
                visual.pig = Some(variant(state, id)?)
            }
            // Correctly framed, schema-checked facts with no appearance consumer in this slice.
            _ => {}
        }
    }
    Ok(())
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
