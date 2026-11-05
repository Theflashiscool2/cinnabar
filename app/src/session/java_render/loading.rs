//! World entry follows current section publication, including legitimate empty sections.
use super::*;
mod cell;

/// Start preparing received spawn terrain without waiting for optional surrounding columns.
pub(super) fn initial_terrain_ready(terrain: &world::java::Terrain, position: [f64; 3]) -> bool {
    let [x, _, z] = position.map(|n| (n.floor() as i32).div_euclid(16));
    terrain.column([x, z]).is_some()
}
impl Presentation {
    /// Native LevelLoadTracker waits on section compilation, not a count of visible draws.
    /// Keep the renderer receipt gate for both empty and nonempty compiled sections.
    pub(super) fn spawn_ready(&self, state: &client_world::java::WorldState) -> bool {
        state.teleport.as_ref().is_some_and(|t| {
            let Some(dimension) = state.terrain.dimension() else {
                return false;
            };
            cell::renderer_cell(dimension, t.motion.position)
                .is_some_and(|cell| self.cell_resident(&state.terrain, cell))
        })
    }
    pub(super) fn waiting_stage(&self, state: &client_world::java::WorldState) -> &'static str {
        if self.failure.is_some() {
            return "failed";
        }
        if self.opened {
            return "ready";
        }
        let Some(position) = &state.teleport else {
            return "awaiting_server_position";
        };
        if !initial_terrain_ready(&state.terrain, position.motion.position) {
            "awaiting_spawn_column"
        } else if self.pending.is_some() {
            "preparing_resources"
        } else if self.stream.is_none() {
            "awaiting_resource_preparation"
        } else if !self.spawn_ready(state) {
            "awaiting_spawn_renderer_receipt"
        } else if state.awaiting_correction_ack() {
            "awaiting_position_acknowledgement"
        } else {
            "ready"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::java::{BiomeId, BlockStateId, Column, Light, Palette, Section};
    fn fixture() -> client_world::java::WorldState {
        let mut state = client_world::java::WorldState::default();
        state.begin(7);
        state.terrain.begin(Dimension {
            session: 7,
            epoch: 1,
            name: "minecraft:overworld".into(),
            min_y: 0,
            height: 16,
            has_skylight: true,
        });
        let section = Arc::new(Section {
            blocks: Palette::new(0, vec![BlockStateId(0)], vec![], 4096).unwrap(),
            biomes: Palette::new(0, vec![BiomeId(0)], vec![], 64).unwrap(),
        });
        assert!(state.terrain.replace(
            [-1, -2],
            Column::new(
                vec![section],
                Light {
                    sky: vec![None; 3],
                    block: vec![None; 3]
                },
                vec![],
                vec![]
            )
        ));
        state.teleport = Some(client_world::java::player::ResolvedTeleport {
            dimension_epoch: 1,
            id: 7,
            revision: 1,
            motion: world::java::player::Motion {
                position: [-0.5, 8., -16.5],
                ..Default::default()
            },
        });
        state
    }
    #[test]
    fn spawn_column_starts_preparation_without_center_or_eight_neighbors() {
        let mut state = fixture();
        assert!(initial_terrain_ready(&state.terrain, [-0.5, 8., -16.5]));
        assert!(!initial_terrain_ready(&state.terrain, [0., 8., -16.5]));
        state.terrain.center = Some([200, 200]);
        assert!(initial_terrain_ready(&state.terrain, [-0.5, 8., -16.5]));
    }
    #[test]
    fn empty_spawn_requires_current_renderer_receipt_and_recovers_after_replacement() {
        let mut state = fixture();
        let mut presentation = Presentation::default();
        assert!(!presentation.spawn_ready(&state));
        assert_eq!(
            presentation.waiting_stage(&state),
            "awaiting_resource_preparation"
        );
        let old = state.terrain.publication_key([-1, -2]).unwrap();
        presentation.resident_keys.insert([-1, 0, -2], old);
        assert!(presentation.uploaded.is_empty()); // No visible geometry is fabricated.
        assert!(presentation.spawn_ready(&state));
        let column = state.terrain.column([-1, -2]).unwrap().as_ref().clone();
        state.terrain.replace([-1, -2], column);
        assert!(!presentation.spawn_ready(&state));
        presentation.resident_keys.insert(
            [-1, 0, -2],
            state.terrain.publication_key([-1, -2]).unwrap(),
        );
        assert!(presentation.spawn_ready(&state));
        let neighbor = state.terrain.column([-1, -2]).unwrap().as_ref().clone();
        state.terrain.replace([-2, -2], neighbor);
        assert!(!presentation.spawn_ready(&state));
        presentation.resident_keys.insert(
            [-1, 0, -2],
            state.terrain.publication_key([-1, -2]).unwrap(),
        );
        assert!(presentation.spawn_ready(&state));
        state.terrain.unload([-1, -2]);
        assert!(!presentation.spawn_ready(&state));
        let mut dimension = state.terrain.dimension().unwrap().clone();
        dimension.epoch += 1;
        state.terrain.begin(dimension);
        assert!(!presentation.spawn_ready(&state));
        state.teleport = None;
        assert_eq!(
            presentation.waiting_stage(&state),
            "awaiting_server_position"
        );
    }
    #[test]
    fn out_of_height_spawn_still_needs_a_current_nearest_section_receipt() {
        let mut state = fixture();
        state.teleport.as_mut().unwrap().motion.position[1] = -60.;
        let mut p = Presentation::default();
        assert!(!p.spawn_ready(&state));
        let old = state.terrain.publication_key([-1, -2]).unwrap();
        p.resident_keys.insert([-1, -4, -2], old.clone());
        assert!(
            !p.spawn_ready(&state),
            "an impossible section is not a receipt for the nearest real one"
        );
        p.resident_keys.insert([-1, 0, -2], old);
        assert!(p.spawn_ready(&state));
        state.teleport.as_mut().unwrap().motion.position[1] = 16.;
        assert!(p.spawn_ready(&state));
        let column = state.terrain.column([-1, -2]).unwrap().as_ref().clone();
        state.terrain.replace([-1, -2], column);
        assert!(
            !p.spawn_ready(&state),
            "stale column publication cannot open an out-of-height spawn"
        );
    }
    #[test]
    fn opened_world_does_not_report_loading_again_on_ordinary_mesh_changes() {
        let mut presentation = Presentation::default();
        let state = fixture();
        presentation.opened = true;
        assert_eq!(presentation.waiting_stage(&state), "ready");
        presentation.failure = Some("fixture resource failure".into());
        assert_eq!(presentation.waiting_stage(&state), "failed");
    }
}
