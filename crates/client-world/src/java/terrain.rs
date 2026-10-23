//! Converts native Java packets into world-owned terrain; no packet types reach meshing.
use super::nbt::convert as nbt;
use super::*;
use java_protocol::{nbt::Value, terrain as wire};
use std::sync::Arc;
use world::java as domain;

#[cfg(test)]
mod tests;

pub(super) fn begin(state: &mut WorldState, spawn: &Spawn) -> Result<(), &'static str> {
    let bootstrap = state
        .bootstrap
        .as_ref()
        .ok_or("terrain before Java bootstrap")?;
    let entry = bootstrap
        .configuration
        .registries
        .get("minecraft:dimension_type")
        .and_then(|r| {
            usize::try_from(spawn.dimension_type)
                .ok()
                .and_then(|i| r.get(i))
        })
        .ok_or("invalid Java dimension ID")?;
    let Value::Compound(fields) = &entry.value else {
        return Err("invalid Java dimension compound");
    };
    let int = |name| match fields.get(name) {
        Some(Value::Int(n)) => Ok(*n),
        _ => Err("missing Java dimension geometry"),
    };
    let has_skylight = match fields.get("has_skylight") {
        Some(Value::Byte(n)) => *n != 0,
        _ => return Err("missing Java dimension skylight flag"),
    };
    let min_y = int("min_y")?;
    let height = int("height")?;
    state.dimension_epoch = state
        .dimension_epoch
        .checked_add(1)
        .ok_or("Java dimension epoch overflow")?;
    state.terrain.begin(domain::Dimension {
        session: state.generation,
        epoch: state.dimension_epoch,
        name: spawn.world.clone(),
        min_y,
        height,
        has_skylight,
    });
    state
        .entities
        .begin(state.generation, state.dimension_epoch);
    state.status.mode = Some(super::hud::mode(
        java_protocol::session::hud::GameMode::from_id(i32::from(spawn.game_mode)),
    ));
    state.status.experience = None;
    state.block_ack = None;
    state.teleport = None;
    state.player_motion = None;
    state.player_dynamics.unplaced_velocity = None;
    state.pending_correction = None;
    state.pending_rotation = None;
    Ok(())
}
fn palette<T: Copy + From<u32>>(p: wire::Palette) -> Result<domain::Palette<T>, &'static str> {
    domain::Palette::new(
        p.bits,
        p.values.into_iter().map(T::from).collect(),
        p.words,
        p.len,
    )
    .ok_or("invalid Java domain palette")
}
fn light(l: wire::Light) -> domain::Light {
    domain::Light {
        sky: l.sky.into_iter().map(|l| l.map(Arc::from)).collect(),
        block: l.block.into_iter().map(|l| l.map(Arc::from)).collect(),
    }
}
pub(super) fn admit(
    state: &mut WorldState,
    epoch: u64,
    packet: wire::Packet,
) -> Result<bool, &'static str> {
    if !state.terrain.accepts(state.generation, epoch) {
        return Ok(false);
    }
    match packet {
        wire::Packet::Chunk(chunk) => {
            let mut sections = Vec::with_capacity(chunk.sections.len());
            for section in chunk.sections {
                sections.push(Arc::new(domain::Section {
                    blocks: palette(section.blocks)?,
                    biomes: palette(section.biomes)?,
                }));
            }
            let mut entities = Vec::with_capacity(chunk.block_entities.len());
            for b in chunk.block_entities {
                let x = chunk
                    .x
                    .checked_mul(16)
                    .and_then(|x| x.checked_add(b.x.into()))
                    .ok_or("Java chunk X overflow")?;
                let z = chunk
                    .z
                    .checked_mul(16)
                    .and_then(|z| z.checked_add(b.z.into()))
                    .ok_or("Java chunk Z overflow")?;
                entities.push(domain::BlockEntity {
                    position: [x, b.y.into(), z],
                    kind: b.kind,
                    data: b.data.map(nbt),
                });
            }
            if !state.terrain.replace(
                [chunk.x, chunk.z],
                domain::Column::new(sections, light(chunk.light), chunk.heightmaps, entities),
            ) {
                return Err("Java chunk geometry disagrees with admitted dimension");
            }
            state.decoded_chunks += 1;
        }
        wire::Packet::Light { x, z, light: l } => {
            state.terrain.update_light([x, z], light(l));
        }
        wire::Packet::Blocks(updates) => {
            for b in updates {
                state
                    .terrain
                    .set_block(b.position, domain::BlockStateId(b.state));
                state.block_updates += 1;
            }
        }
        wire::Packet::Unload { x, z } => state.terrain.unload([x, z]),
        wire::Packet::Center { x, z } => state.terrain.center = Some([x, z]),
        wire::Packet::Radius(n) => state.terrain.radius = Some(n),
        wire::Packet::Biomes(chunks) => {
            for (x, z, biomes) in chunks {
                state.terrain.update_biomes(
                    [x, z],
                    biomes.into_iter().map(palette).collect::<Result<_, _>>()?,
                );
            }
        }
    }
    Ok(true)
}
