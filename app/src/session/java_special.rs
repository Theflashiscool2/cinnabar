//! Native block-entity extraction into the shared block-entity GPU frame.
//! Bedrock retains its own scene/artwork. No Java ID is interpreted as a Bedrock state.
use assets::registry::java::{
    graphics::Graphics,
    special::{Chest, ChestHalf},
};
use bevy::prelude::*;
use render_model::block_entity::java_chest::Model;
use render_model::{
    BlockEntityArtworkPreparer, BlockEntityFrame, BlockEntityKind, BlockEntityLight,
    BlockEntityScene, BlockEntitySubmission, PreparedBlockEntityArtwork, SceneClock,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use world::java::{Dimension, Terrain};

type Pending = crossbeam_channel::Receiver<([u8; 32], Result<PreparedBlockEntityArtwork, String>)>;
#[derive(Default, Resource)]
pub(crate) struct Presentation {
    scope: Option<Dimension>,
    identity: Option<[u8; 32]>,
    pending: Option<Pending>,
    scene: BlockEntityScene,
    graphics: Option<Arc<Graphics>>,
    columns: BTreeMap<[i32; 2], (u64, Vec<([i32; 3], u32)>)>,
    lids: BTreeMap<[i32; 3], (u64, f32)>,
}

/// Resolves the target's half model only with a resident compatible partner. Both halves draw
/// their own texture/model, never a duplicate whole double chest.
fn half(
    terrain: &Terrain,
    graphics: &Graphics,
    p: [i32; 3],
    chest: &Chest,
) -> (ChestHalf, Option<[i32; 3]>) {
    let partner = chest.partner_offset().and_then(|offset| {
        let position = std::array::from_fn(|i| p[i] + offset[i]);
        terrain
            .block(position)
            .and_then(|id| graphics.chests.get(&id.0))
            .filter(|other| chest.pairs_with(other))
            .map(|_| position)
    });
    (
        if partner.is_some() {
            chest.half
        } else {
            ChestHalf::Single
        },
        partner,
    )
}

pub(crate) fn update(
    controller: Option<Res<super::SessionController>>,
    mut terrain_presentation: ResMut<super::java_render::Presentation>,
    mut presentation: ResMut<Presentation>,
    frame: Option<ResMut<render_scene::BlockEntityFrameResource>>,
    time: Res<Time<Real>>,
) {
    let Some(mut frame) = frame else {
        return;
    };
    let active = controller
        .as_ref()
        .and_then(|c| c.java.as_ref())
        .and_then(|_| terrain_presentation.special_inputs());
    let p = &mut *presentation;
    let Some((dimension, graphics)) = active else {
        if p.scope.is_some() {
            *p = Presentation::default();
            frame.0 = BlockEntityFrame::default();
        }
        return;
    };
    if p.scope.as_ref() != Some(&dimension) {
        *p = Presentation {
            scope: Some(dimension.clone()),
            ..Default::default()
        };
        frame.0 = BlockEntityFrame::default();
    }
    let Some(atlas) = graphics.special_atlas.as_ref() else {
        return;
    };
    let identity = atlas.identity();
    if let Some(receiver) = p.pending.as_ref() {
        match receiver.try_recv() {
            Ok((requested, result)) => {
                p.pending = None;
                if requested == identity {
                    match result {
                        Ok(artwork) => {
                            p.scene.install_prepared_assets(&artwork);
                            p.identity = Some(identity);
                        }
                        Err(error) => {
                            terrain_presentation.special_failure(error);
                            return;
                        }
                    }
                }
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                p.pending = None;
                terrain_presentation
                    .special_failure("Java block-entity artwork worker stopped".into());
                return;
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
        }
    }
    if p.identity != Some(identity) {
        if p.pending.is_none() {
            let atlas = Arc::clone(atlas);
            let (sender, receiver) = crossbeam_channel::bounded(1);
            p.pending = Some(receiver);
            if let Err(error) = std::thread::Builder::new()
                .name("java-special-artwork".into())
                .spawn(move || {
                    let prepared = BlockEntityArtworkPreparer::default()
                        .prepare(&atlas)
                        .ok_or_else(|| "Java block-entity atlas rejected by renderer".into());
                    let _ = sender.send((identity, prepared));
                })
            {
                p.pending = None;
                terrain_presentation.special_failure(error.to_string());
            }
        }
        // Never publish geometry against an atlas from an obsolete resource generation.
        frame.0 = BlockEntityFrame::default();
        return;
    }
    if p.graphics
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, &graphics))
    {
        p.columns.clear();
        p.graphics = Some(Arc::clone(&graphics));
    }
    let controller = controller.as_ref().expect("active session");
    let terrain = &controller.java_world.terrain;
    let resident: BTreeSet<_> = terrain.columns().map(|(key, _)| key).collect();
    p.columns.retain(|key, _| resident.contains(key));
    let mut budget = 4;
    for (key, column) in terrain.columns() {
        if p.columns
            .get(&key)
            .is_some_and(|(revision, _)| *revision == column.revision())
        {
            continue;
        }
        // Changed positions cannot retain old geometry while a bounded rescan is pending.
        p.columns.remove(&key);
        if budget == 0 {
            continue;
        }
        budget -= 1;
        let mut chests = Vec::new();
        for (section_index, section) in column.sections.iter().enumerate() {
            for index in 0..4096 {
                let Some(id) = section
                    .blocks
                    .get(index)
                    .filter(|id| graphics.chests.contains_key(&id.0))
                else {
                    continue;
                };
                let position = [
                    key[0] * 16 + (index % 16) as i32,
                    dimension.min_y + section_index as i32 * 16 + (index / 256) as i32,
                    key[1] * 16 + (index / 16 % 16) as i32,
                ];
                chests.push((position, id.0));
            }
        }
        p.columns.insert(key, (column.revision(), chests));
    }
    let mut submissions = Vec::new();
    let mut alive = BTreeSet::new();
    let step = time.delta_secs().min(0.25) * 2.; // ChestLidController: 0.1 per 20Hz tick.
    for (_, positions) in p.columns.values() {
        for &(position, id) in positions {
            let chest = &graphics.chests[&id];
            let (half, partner) = half(terrain, &graphics, position, chest);
            let open = |pos| {
                terrain
                    .block_event(pos)
                    .is_some_and(|e| e.block == chest.block_id && e.action == 1 && e.value > 0)
            };
            let target = f32::from(open(position) || partner.is_some_and(open));
            let generation = terrain.block_generation(position).expect("resident chest");
            let entry = p.lids.entry(position).or_insert((generation, 0.));
            if entry.0 != generation {
                *entry = (generation, 0.);
            }
            entry.1 += (target - entry.1).clamp(-step, step);
            alive.insert(position);
            let Some(mut light) = terrain.light(position) else {
                continue;
            };
            if let Some(other) = partner.and_then(|pos| terrain.light(pos)) {
                light = [light[0].max(other[0]), light[1].max(other[1])];
            }
            submissions.push(BlockEntitySubmission {
                block: position,
                light: BlockEntityLight::Actor {
                    block: light[0],
                    sky: light[1],
                },
                kind: BlockEntityKind::JavaChest(Model {
                    state: chest.clone(),
                    half,
                    lid: entry.1,
                }),
            });
        }
    }
    p.lids.retain(|pos, _| alive.contains(pos));
    frame.0 = p
        .scene
        .update(
            SceneClock {
                ticks: time.elapsed_secs_f64() * 20.,
            },
            &[],
            &submissions,
        )
        .clone();
}

impl Presentation {
    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({"session":self.scope.as_ref().map(|s|s.session),"artwork_pending":self.pending.is_some(),
            "cached_columns":self.columns.len(),"lids":self.lids.iter().take(16).map(|(pos,(generation,lid))|serde_json::json!({"position":pos,"generation":generation,"lid":lid})).collect::<Vec<_>>()})
    }
}
