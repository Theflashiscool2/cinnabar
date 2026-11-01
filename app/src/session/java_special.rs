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
    BlockEntityScene, BlockEntitySubmission, SceneClock,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use world::java::{Dimension, Terrain};

mod artwork;
use artwork::Pending;
#[derive(Default, Resource)]
pub(crate) struct Presentation {
    scope: Option<Dimension>,
    identity: Option<[u8; 32]>,
    pending: Option<Pending>,
    scene: BlockEntityScene,
    graphics: Option<Arc<Graphics>>,
    columns: BTreeMap<[i32; 2], (u64, Vec<([i32; 3], u32)>)>,
    banners: BTreeMap<[i32; 3], render_model::block_entity::java_banner::Model>,
    pots: BTreeMap<[i32; 3], render_model::block_entity::java_pot::Model>,
    gateways: BTreeMap<[i32; 3], (u64, sim::java::gateway::Motion)>,
    conduits: BTreeMap<[i32; 3], (u64, sim::java::conduit::Motion)>,
    pot_motion: BTreeMap<[i32; 3], (u64, sim::java::pot::Motion)>,
    lids: BTreeMap<[i32; 3], (u64, f32)>,
    boxes: BTreeMap<[i32; 3], BoxMotion>,
    skulls: BTreeMap<[i32; 3], (u64, sim::java::skull::Motion)>,
    heads: BTreeMap<[i32; 3], super::java_heads::Head>,
    bound_base: Option<[u8; 32]>,
    bound_skins: BTreeSet<String>,
    elapsed: std::time::Duration,
}

struct BoxMotion {
    generation: u64,
    block_id: u32,
    motion: sim::java::shulker::Motion,
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

/// Advances interaction authority before NetworkSend, independently of render publication.
pub(crate) fn advance(
    time: Res<Time<Real>>,
    controller: Option<Res<super::SessionController>>,
    mut presentation: ResMut<Presentation>,
) {
    let p = &mut *presentation;
    let terrain = controller
        .as_ref()
        .filter(|c| c.java.is_some())
        .map(|c| &c.java_world.terrain);
    let Some(terrain) = terrain.filter(|t| p.scope.is_some() && p.scope.as_ref() == t.dimension())
    else {
        p.boxes.clear();
        p.skulls.clear();
        p.pot_motion.clear();
        p.conduits.clear();
        p.gateways.clear();
        p.elapsed = std::time::Duration::ZERO;
        return;
    };
    let Some(graphics) = p.graphics.as_ref() else {
        return;
    };
    p.elapsed = p.elapsed.saturating_add(time.delta());
    let mut ticks = 0;
    while p.elapsed >= simulation_clock::TICK_DURATION
        && ticks < simulation_clock::MAX_TICKS_PER_FRAME
    {
        p.elapsed -= simulation_clock::TICK_DURATION;
        ticks += 1;
    }
    p.conduits
        .retain(|pos, (generation, _)| terrain.block_generation(*pos) == Some(*generation));
    p.boxes
        .retain(|pos, motion| terrain.block_generation(*pos) == Some(motion.generation));
    p.skulls
        .retain(|pos, (generation, _)| terrain.block_generation(*pos) == Some(*generation));
    p.pot_motion
        .retain(|pos, (generation, _)| terrain.block_generation(*pos) == Some(*generation));
    p.gateways
        .retain(|pos, (generation, _)| terrain.block_generation(*pos) == Some(*generation));
    for (pos, (_, motion)) in &mut p.gateways {
        for _ in 0..ticks {
            motion.tick();
        }
        if let Some(event) = terrain.block_event(*pos).filter(|e| {
            e.action == 1
                && terrain
                    .block(*pos)
                    .and_then(|id| graphics.block_types.get(id.0 as usize))
                    .copied()
                    == Some(e.block)
        }) {
            motion.event(event.revision);
        }
    }
    for (position, model) in &p.pots {
        let Some(generation) = terrain.block_generation(*position) else {
            continue;
        };
        let (_, motion) = p
            .pot_motion
            .entry(*position)
            .or_insert_with(|| (generation, Default::default()));
        for _ in 0..ticks {
            motion.tick();
        }
        if let Some(event) = terrain
            .block_event(*position)
            .filter(|e| e.block == model.state.block_id && e.action == 1)
        {
            motion.event(event.revision, event.value);
        }
    }
    for (_, positions) in p.columns.values() {
        for &(position, id) in positions {
            if graphics.conduits.contains(&id) {
                if terrain.block(position).map(|n| n.0) != Some(id) {
                    continue;
                }
                let Some(generation) = terrain.block_generation(position) else {
                    continue;
                };
                let (_, motion) = p
                    .conduits
                    .entry(position)
                    .or_insert_with(|| (generation, Default::default()));
                let frame = controller
                    .as_ref()
                    .and_then(|c| c.java_world.bootstrap.as_ref())
                    .and_then(|b| b.configuration.tags.get("minecraft:block"))
                    .and_then(|t| t.get(assets::registry::java::conduit::EFFECT_TAG));
                for _ in 0..ticks {
                    motion.tick(|offset| {
                        let id = terrain
                            .block(std::array::from_fn(|i| position[i] + offset[i]))?
                            .0;
                        let water = graphics.fluid_facts.as_ref()?.state(id)?.kind
                            == assets::registry::java::fluids::Kind::Water;
                        let block = *graphics.block_types.get(id as usize)?;
                        Some((
                            water,
                            frame.is_some_and(|ids| ids.contains(&(block as i32))),
                        ))
                    });
                }
                continue;
            }
            if let Some(state) = graphics.skulls.get(&id) {
                if terrain.block(position).map(|n| n.0) == Some(id) {
                    if let Some(generation) = terrain.block_generation(position) {
                        let (_, motion) = p
                            .skulls
                            .entry(position)
                            .or_insert_with(|| (generation, Default::default()));
                        motion.set_powered(state.powered);
                        for _ in 0..ticks {
                            motion.tick();
                        }
                    }
                }
                continue;
            }
            let Some(state) = graphics.shulkers.get(&id) else {
                continue;
            };
            if terrain.block(position).map(|id| id.0) != Some(id) {
                continue;
            }
            let Some(generation) = terrain.block_generation(position) else {
                continue;
            };
            p.boxes.entry(position).or_insert_with(|| BoxMotion {
                generation,
                block_id: state.block_id,
                motion: Default::default(),
            });
        }
    }
    // Known hosts keep ticking while a newly received facing awaits graphics admission.
    for (position, state) in &mut p.boxes {
        if let Some(event) = terrain
            .block_event(*position)
            .filter(|e| e.block == state.block_id && e.action == 1)
        {
            state.motion.event(event.binary_transition);
        }
        for _ in 0..ticks {
            state.motion.tick();
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Bevy world, resource-publication and scoped-cache system inputs"
)]
pub(crate) fn update(
    controller: Option<Res<super::SessionController>>,
    mut terrain_presentation: ResMut<super::java_render::Presentation>,
    mut presentation: ResMut<Presentation>,
    frame: Option<ResMut<render_scene::BlockEntityFrameResource>>,
    time: Res<Time<Real>>,
    view: Option<Res<client_presentation::local_player::LocalViewPose>>,
    camera: Option<Res<client_presentation::local_player::CameraPose>>,
    mut skins: Local<client_shell::player_skin::java::cache::Cache>,
    mut profiles: Local<client_shell::player_skin::java::profiles::Cache>,
) {
    let Some(mut frame) = frame else {
        return;
    };
    let active = controller
        .as_ref()
        .and_then(|c| c.java.as_ref())
        .and_then(|_| terrain_presentation.special_inputs());
    let p = &mut *presentation;
    let Some((dimension, graphics)) = active.filter(|(d, _)| {
        controller
            .as_ref()
            .is_some_and(|c| c.java_world.terrain.dimension() == Some(d))
    }) else {
        skins.scope(None);
        profiles.scope(None);
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
    if p.graphics
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, &graphics))
    {
        p.columns.clear();
        p.banners.clear();
        p.pots.clear();
        p.heads.clear();
        p.graphics = Some(Arc::clone(&graphics));
    }
    let controller = controller.as_ref().expect("active session");
    let terrain = &controller.java_world.terrain;
    let resident: BTreeSet<_> = terrain.columns().map(|(key, _)| key).collect();
    p.columns.retain(|key, _| resident.contains(key));
    p.banners
        .retain(|pos, _| resident.contains(&[pos[0].div_euclid(16), pos[2].div_euclid(16)]));
    p.pots
        .retain(|pos, _| resident.contains(&[pos[0].div_euclid(16), pos[2].div_euclid(16)]));
    p.heads.retain(|pos, head| head.current(terrain, *pos));
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
        p.banners
            .retain(|pos, _| [pos[0].div_euclid(16), pos[2].div_euclid(16)] != key);
        p.pots
            .retain(|pos, _| [pos[0].div_euclid(16), pos[2].div_euclid(16)] != key);
        if budget == 0 {
            continue;
        }
        budget -= 1;
        let mut chests = Vec::new();
        for (section_index, section) in column.sections.iter().enumerate() {
            for index in 0..4096 {
                let Some(id) = section.blocks.get(index).filter(|id| {
                    graphics.chests.contains_key(&id.0)
                        || graphics.shulkers.contains_key(&id.0)
                        || graphics.banners.contains_key(&id.0)
                        || graphics.skulls.contains_key(&id.0)
                        || graphics.statues.contains_key(&id.0)
                        || graphics.pots.contains_key(&id.0)
                        || graphics.conduits.contains(&id.0)
                        || graphics.portals.contains(&id.0)
                        || graphics.gateways.contains(&id.0)
                }) else {
                    continue;
                };
                let position = [
                    key[0] * 16 + (index % 16) as i32,
                    dimension.min_y + section_index as i32 * 16 + (index / 256) as i32,
                    key[1] * 16 + (index / 16 % 16) as i32,
                ];
                if graphics.gateways.contains(&id.0) {
                    let data = column
                        .block_entities
                        .iter()
                        .find(|e| e.position == position)
                        .and_then(|e| e.data.as_ref());
                    let age = match render_model::block_entity::java_gateway::age(data) {
                        Ok(age) => age,
                        Err(error) => {
                            terrain_presentation.special_failure(error);
                            return;
                        }
                    };
                    if let Some(generation) = terrain.block_generation(position) {
                        let entry = p
                            .gateways
                            .entry(position)
                            .or_insert_with(|| (generation, Default::default()));
                        if entry.0 != generation {
                            *entry = (generation, Default::default());
                        }
                        entry.1.synchronize(
                            age,
                            terrain
                                .block_entity_revision(position)
                                .unwrap_or(generation),
                        );
                    }
                    chests.push((position, id.0));
                } else if graphics
                    .skulls
                    .get(&id.0)
                    .is_some_and(|s| s.shape == assets::registry::java::skull::Shape::Player)
                {
                    if !p.heads.contains_key(&position) {
                        let data = column
                            .block_entities
                            .iter()
                            .find(|e| e.position == position)
                            .and_then(|e| e.data.as_ref());
                        match super::java_heads::Head::new(terrain, position, data) {
                            Ok(head) => {
                                p.heads.insert(position, head);
                            }
                            Err(error) => {
                                terrain_presentation.special_failure(error);
                                return;
                            }
                        }
                    }
                    chests.push((position, id.0));
                } else if let Some(state) = graphics.pots.get(&id.0) {
                    let data = column
                        .block_entities
                        .iter()
                        .find(|e| e.position == position)
                        .and_then(|e| e.data.as_ref());
                    match render_model::block_entity::java_pot::Model::from_data(
                        state.clone(),
                        data,
                        &graphics.pot_patterns,
                        &graphics.special_models,
                    ) {
                        Ok(model) => {
                            p.pots.insert(position, model);
                        }
                        Err(error) => {
                            terrain_presentation.special_failure(error);
                            return;
                        }
                    }
                } else if let Some(state) = graphics.banners.get(&id.0) {
                    let data = column
                        .block_entities
                        .iter()
                        .find(|e| e.position == position)
                        .and_then(|e| e.data.as_ref());
                    match render_model::block_entity::java_banner::Model::from_data(
                        state.clone(),
                        data,
                        &graphics.banner_patterns,
                    ) {
                        Ok(model) => {
                            p.banners.insert(position, model);
                        }
                        Err(error) => {
                            terrain_presentation.special_failure(error);
                            return;
                        }
                    }
                } else {
                    chests.push((position, id.0));
                }
            }
        }
        p.columns.insert(key, (column.revision(), chests));
    }
    let queries = p.heads.values().filter_map(|h| h.query.clone()).collect();
    profiles.update(
        (dimension.session, dimension.epoch),
        time.elapsed(),
        &queries,
    );
    for head in p.heads.values_mut() {
        head.resolve(&profiles);
    }
    let desired = p
        .columns
        .values()
        .flat_map(|(_, positions)| positions.iter())
        .filter_map(|(pos, _)| p.heads.get(pos).and_then(|h| h.texture.clone()))
        .collect();
    skins.update(
        (dimension.session, dimension.epoch),
        time.elapsed(),
        &desired,
    );
    for (pos, head) in &mut p.heads {
        head.report_failure(*pos, &skins, &profiles);
    }
    let images = skins.images(&desired);
    let patches: Vec<_> = p
        .columns
        .values()
        .flat_map(|(_, positions)| positions.iter())
        .filter_map(|(pos, _)| p.heads.get(pos).and_then(|h| h.patch.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let keys: BTreeSet<_> = images
        .keys()
        .cloned()
        .chain(patches.iter().map(super::java_heads::patch_key))
        .collect();
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new()
        .chain_update(atlas.identity())
        .chain_update(skins.identity(&desired));
    for key in &keys {
        hash.update((key.len() as u64).to_be_bytes());
        hash.update(key.as_bytes());
    }
    let identity: [u8; 32] = hash.finalize().into();
    if p.pending.as_ref().is_some_and(|r| r.identity != identity) {
        p.pending = None;
    }
    if let Some(receiver) = p.pending.as_ref() {
        match receiver.try_recv() {
            Ok((requested, result)) => {
                p.pending = None;
                if requested == identity {
                    match result {
                        Ok(artwork) => {
                            p.scene.install_prepared_assets(&artwork);
                            p.identity = Some(identity);
                            p.bound_base = Some(atlas.identity());
                            p.bound_skins = keys;
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
        if p.pending.is_none()
            && let Some(lease) = artwork::Lease::acquire()
        {
            let atlas = Arc::clone(atlas);
            let mut images = images.clone();
            let (sender, receiver) = crossbeam_channel::bounded(1);
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            p.pending = Some(Pending::new(receiver, Arc::clone(&cancel), identity));
            if let Err(error) = std::thread::Builder::new()
                .name("java-special-artwork".into())
                .spawn(move || {
                    let _lease = lease;
                    let prepared =
                        super::java_heads::prepare_patches(&mut images, &patches, &cancel)
                            .and_then(|()| {
                                BlockEntityArtworkPreparer::default()
                                    .with_java_skins(&images)
                                    .map_err(str::to_owned)
                                    .and_then(|preparer| {
                                        preparer.prepare(&atlas).ok_or_else(|| {
                                            "Java block-entity atlas rejected by renderer".into()
                                        })
                                    })
                            });
                    if !cancel.load(std::sync::atomic::Ordering::Acquire) {
                        let _ = sender.send((identity, prepared));
                    }
                })
            {
                p.pending = None;
                terrain_presentation.special_failure(error.to_string());
            }
        }
        // Never publish geometry against an atlas from an obsolete resource generation.
        if p.bound_base != Some(atlas.identity()) {
            frame.0 = BlockEntityFrame::default();
            return;
        }
    }
    let mut submissions = Vec::new();
    let mut alive = BTreeSet::new();
    let partial =
        (p.elapsed.as_secs_f64() / simulation_clock::TICK_DURATION.as_secs_f64()).min(1.) as f32;
    let step = time.delta_secs().min(0.25) * 2.; // ChestLidController: 0.1 per 20Hz tick.
    for (_, positions) in p.columns.values() {
        for &(position, id) in positions {
            if graphics.gateways.contains(&id) {
                if let Some(resources) = &graphics.portal {
                    let offsets = [
                        [-1, 0, 0],
                        [1, 0, 0],
                        [0, -1, 0],
                        [0, 1, 0],
                        [0, 0, -1],
                        [0, 0, 1],
                    ];
                    let mut faces = 0;
                    for (face, offset) in offsets.iter().enumerate() {
                        let neighbor =
                            terrain.block(std::array::from_fn(|i| position[i] + offset[i]));
                        let hidden = neighbor
                            .and_then(|id| graphics.fluid_facts.as_ref()?.state(id.0))
                            .is_some_and(|s| s.full_face & (1 << face) != 0);
                        if !hidden {
                            faces |= 1 << face;
                        }
                    }
                    let beam = p
                        .gateways
                        .get(&position)
                        .and_then(|(_, m)| {
                            m.sample(partial, dimension.min_y + dimension.height - 1)
                        })
                        .map(|v| render_model::block_entity::java_gateway::Beam {
                            height: v.height,
                            scale: v.scale,
                            spawning: v.spawning,
                        });
                    submissions.push(BlockEntitySubmission {
                        block: position,
                        light: BlockEntityLight::Scalar(1.),
                        kind: BlockEntityKind::JavaGateway(
                            render_model::block_entity::java_gateway::Model {
                                resources: Arc::clone(resources),
                                faces,
                                beam,
                            },
                        ),
                    });
                }
                continue;
            }
            if graphics.portals.contains(&id) {
                if let Some(resources) = &graphics.portal {
                    submissions.push(BlockEntitySubmission {
                        block: position,
                        light: BlockEntityLight::Scalar(1.),
                        kind: BlockEntityKind::JavaPortal(
                            render_model::block_entity::java_portal::Model {
                                resources: Arc::clone(resources),
                            },
                        ),
                    });
                }
                continue;
            }
            if graphics.conduits.contains(&id) {
                if let (Some(resources), Some([block, sky])) =
                    (graphics.conduit.as_ref(), terrain.light(position))
                {
                    let motion = p.conduits.get(&position).map(|(_, m)| m);
                    let shape = motion.map_or(Default::default(), |m| m.shape);
                    let (ticks, rotation) = motion.map_or((0., 0.), |m| m.sample(partial));
                    submissions.push(BlockEntitySubmission {
                        block: position,
                        light: BlockEntityLight::Actor { block, sky },
                        kind: BlockEntityKind::JavaConduit(
                            render_model::block_entity::java_conduit::Model {
                                resources: Arc::clone(resources),
                                active: shape.active,
                                hunting: shape.hunting,
                                ticks,
                                texture_ticks: time.elapsed_secs_f64() * 20.,
                                rotation,
                                camera: view
                                    .as_ref()
                                    .map_or(Quat::IDENTITY, |v| v.camera_rotation()),
                            },
                        ),
                    });
                }
                continue;
            }
            if let Some(state) = graphics.statues.get(&id) {
                if let Some([block, sky]) = terrain.light(position) {
                    submissions.push(BlockEntitySubmission {
                        block: position,
                        light: BlockEntityLight::Actor { block, sky },
                        kind: BlockEntityKind::JavaStatue(
                            render_model::block_entity::java_statue::Model {
                                state: state.clone(),
                                root: Arc::clone(&graphics.special_models[state.model]),
                            },
                        ),
                    });
                }
                continue;
            }
            if let Some(state) = graphics.skulls.get(&id) {
                if let Some([block, sky]) = terrain.light(position) {
                    submissions.push(BlockEntitySubmission {
                        block: position,
                        light: BlockEntityLight::Actor { block, sky },
                        kind: BlockEntityKind::JavaSkull(
                            render_model::block_entity::java_skull::Model {
                                state: state.clone(),
                                texture: p
                                    .heads
                                    .get(&position)
                                    .map(|h| h.atlas_texture(&p.bound_skins)),
                                animation: p
                                    .skulls
                                    .get(&position)
                                    .map_or(0., |(_, m)| m.sample(partial)),
                            },
                        ),
                    });
                }
                continue;
            }
            let chest = graphics.chests.get(&id);
            let (block_id, half, partner) = if let Some(chest) = chest {
                let (half, partner) = half(terrain, &graphics, position, chest);
                (chest.block_id, half, partner)
            } else {
                (graphics.shulkers[&id].block_id, ChestHalf::Single, None)
            };
            let open = |pos| {
                terrain
                    .block_event(pos)
                    .is_some_and(|e| e.block == block_id && e.action == 1 && e.value > 0)
            };
            let target = f32::from(open(position) || partner.is_some_and(open));
            let generation = terrain
                .block_generation(position)
                .expect("resident special block");
            let entry = p.lids.entry(position).or_insert((generation, 0.));
            if entry.0 != generation {
                *entry = (generation, 0.);
            }
            entry.1 += (target - entry.1).clamp(-step, step);
            if chest.is_none() {
                entry.1 = p
                    .boxes
                    .get(&position)
                    .filter(|m| m.generation == generation)
                    .map_or(0., |m| m.motion.sample(partial));
            }
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
                kind: if let Some(chest) = chest {
                    BlockEntityKind::JavaChest(Model {
                        state: chest.clone(),
                        half,
                        lid: entry.1,
                    })
                } else {
                    BlockEntityKind::JavaShulker(render_model::block_entity::java_shulker::Model {
                        state: graphics.shulkers[&id].clone(),
                        open: entry.1,
                    })
                },
            });
        }
    }
    for (position, model) in &p.banners {
        let Some([block, sky]) = terrain.light(*position) else {
            continue;
        };
        submissions.push(BlockEntitySubmission {
            block: *position,
            light: BlockEntityLight::Actor { block, sky },
            kind: BlockEntityKind::JavaBanner(model.clone()),
        });
    }
    for (position, model) in &p.pots {
        let Some([block, sky]) = terrain.light(*position) else {
            continue;
        };
        let mut model = model.clone();
        model.wobble = p
            .pot_motion
            .get(position)
            .and_then(|(_, m)| m.sample(partial));
        submissions.push(BlockEntitySubmission {
            block: *position,
            light: BlockEntityLight::Actor { block, sky },
            kind: BlockEntityKind::JavaPot(model),
        });
    }
    p.lids.retain(|pos, _| alive.contains(pos));
    p.boxes
        .retain(|pos, motion| terrain.block_generation(*pos) == Some(motion.generation));
    p.skulls
        .retain(|pos, (generation, _)| terrain.block_generation(*pos) == Some(*generation));
    if let Some(camera) = camera {
        p.scene
            .set_camera_position(camera.transform().translation.to_array());
    }
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
    /// The picker reads tick state, independently of the interpolated GPU shell.
    pub(crate) fn outline(
        &self,
        terrain: &Terrain,
        position: [i32; 3],
        id: u32,
    ) -> Option<assets::registry::java::physics::Bounds> {
        if self.scope.as_ref() != terrain.dimension() {
            return None;
        }
        let state = self.graphics.as_ref()?.shulkers.get(&id)?;
        let state_motion = self.boxes.get(&position)?;
        if terrain.block_generation(position) != Some(state_motion.generation) {
            return None;
        }
        state_motion.motion.outline(state.facing)
    }
    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({"session":self.scope.as_ref().map(|s|s.session),"artwork_pending":self.pending.is_some(),
            "gateways":self.gateways.iter().take(16).map(|(pos,(generation,m))|serde_json::json!({"position":pos,"generation":generation,"beam":m.sample(0.,self.scope.as_ref().map_or(0,|d|d.min_y+d.height-1)).map(|b|serde_json::json!({"spawning":b.spawning,"height":b.height,"scale":b.scale}))})).collect::<Vec<_>>(),
            "conduits":self.conduits.iter().take(16).map(|(pos,(_,m))|serde_json::json!({"position":pos,"active":m.shape.active,"hunting":m.shape.hunting,"frame_blocks":m.shape.frame_blocks})).collect::<Vec<_>>(),"cached_columns":self.columns.len(),"player_heads":self.heads.len(),"bound_head_textures":self.bound_skins.len(),"pots":self.pots.len(),"pot_wobble":self.pot_motion.iter().filter_map(|(pos,(_,m))|m.sample(0.).map(|w|serde_json::json!({"position":pos,"style":w.0,"progress":w.1}))).take(16).collect::<Vec<_>>(),"banners":self.banners.len(),"skulls":self.skulls.len(),"statues":self.columns.values().flat_map(|(_,p)|p).filter(|(_,id)|self.graphics.as_ref().is_some_and(|g|g.statues.contains_key(id))).count(),"skull_phases":self.skulls.iter().take(16).map(|(pos,(generation,motion))|serde_json::json!({"position":pos,"generation":generation,"phase":motion.sample(0.)})).collect::<Vec<_>>(),"lids":self.lids.iter().take(16).map(|(pos,(generation,lid))|serde_json::json!({"position":pos,"generation":generation,"lid":lid})).collect::<Vec<_>>()})
    }
}
