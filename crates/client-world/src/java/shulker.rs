//! Synchronous native lid events; NBT updates retain the existing block entity's progress.
use world::java::{Terrain, shulker::State};

pub(super) fn event(
    terrain: &mut Terrain,
    position: [i32; 3],
    block: u32,
    action: u8,
    viewers: u8,
) {
    if action != 1 {
        return;
    }
    let Some((expected, _)) = terrain
        .block(position)
        .and_then(|id| assets::registry::java::physics::dynamic::shulker_identity(id.0))
    else {
        return;
    };
    if block != expected {
        return;
    }
    let Some(states) = assets::registry::java::physics::dynamic::shulker_states(block) else {
        return;
    };
    let Some(generation) = terrain.block_generation(position) else {
        return;
    };
    let state = terrain.shulkers.entry(position).or_insert_with(|| State {
        generation,
        block_id: block,
        states: states.clone(),
        motion: Default::default(),
    });
    if state.generation != generation || state.block_id != block {
        *state = State {
            generation,
            block_id: block,
            states,
            motion: Default::default(),
        };
    }
    state.motion.event(viewers);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use world::java::{BiomeId, BlockStateId, Column, Light, Palette, Section};
    fn scene() -> (super::super::WorldState, u32, u32) {
        let mut state = super::super::tests_support::state();
        state.terrain.replace(
            [0, 0],
            Column::new(
                (0..24)
                    .map(|_| {
                        Arc::new(Section {
                            blocks: Palette::uniform(BlockStateId(0), 4096),
                            biomes: Palette::uniform(BiomeId(0), 64),
                        })
                    })
                    .collect(),
                Light {
                    sky: vec![None; 26],
                    block: vec![None; 26],
                },
                vec![],
                vec![],
            ),
        );
        let id = (0..java_protocol::target::BLOCK_STATE_COUNT as u32)
            .find(|id| assets::registry::java::physics::dynamic::shulker_identity(*id).is_some())
            .unwrap();
        let (block, _) = assets::registry::java::physics::dynamic::shulker_identity(id).unwrap();
        state.terrain.set_block([8, 0, 8], BlockStateId(id));
        (state, id, block)
    }
    fn admit(state: &mut super::super::WorldState, block: u32, action: u8, value: u8) {
        super::super::terrain::admit(
            state,
            1,
            java_protocol::terrain::Packet::BlockEvent {
                position: [8, 0, 8],
                block,
                action,
                value,
            },
        )
        .unwrap();
    }
    #[test]
    fn events_are_synchronous_ticks_are_explicit_and_facing_or_nbt_preserves_progress() {
        let (mut state, id, block) = scene();
        admit(&mut state, block + 1, 1, 1);
        admit(&mut state, block, 2, 1);
        assert!(state.terrain.shulkers.is_empty());
        admit(&mut state, block, 1, 1);
        assert_eq!(state.terrain.shulkers[&[8, 0, 8]].motion.sample(1.), 0.);
        for _ in 0..4 {
            state.tick_shulkers();
        }
        let before = state.terrain.shulkers[&[8, 0, 8]].motion.sample(1.);
        assert!(before > 0.);
        let generation = state.terrain.block_generation([8, 0, 8]);
        let siblings = assets::registry::java::physics::dynamic::shulker_states(block).unwrap();
        let other = *siblings.iter().find(|other| **other != id).unwrap();
        state.terrain.set_block([8, 0, 8], BlockStateId(other));
        assert_eq!(
            state.terrain.block_generation([8, 0, 8]),
            generation,
            "same block facing before graphics installation"
        );
        state.terrain.set_block_entity(world::java::BlockEntity {
            position: [8, 0, 8],
            kind: 0,
            data: None,
        });
        assert_eq!(state.terrain.shulkers[&[8, 0, 8]].motion.sample(1.), before);
        admit(&mut state, block, 1, 3);
        state.tick_shulkers();
        assert!(
            state.terrain.shulkers[&[8, 0, 8]].motion.sample(1.) > before,
            "higher viewer counts preserve opening"
        );
        admit(&mut state, block, 1, 0);
        for _ in 0..12 {
            state.tick_shulkers();
        }
        assert_eq!(state.terrain.shulkers[&[8, 0, 8]].motion.sample(1.), 0.);
    }
    #[test]
    fn replacement_unload_and_dimension_reset_retire_logical_lids() {
        for change in 0..4 {
            let (mut state, _, block) = scene();
            admit(&mut state, block, 1, 1);
            state.tick_shulkers();
            match change {
                0 => state.terrain.set_block([8, 0, 8], BlockStateId(0)),
                1 => state.terrain.unload([0, 0]),
                2 => {
                    let column = (**state.terrain.column([0, 0]).unwrap()).clone();
                    state.terrain.replace([0, 0], column);
                }
                _ => {
                    let dimension = state.terrain.dimension().unwrap().clone();
                    state.terrain.begin(dimension);
                }
            }
            assert!(state.terrain.shulkers.is_empty());
        }
    }
}
