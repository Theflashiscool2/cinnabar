//! Binds admitted native entity facts to the common actor scene for this frame.
use super::*;
fn empty() -> ActorPresentationBatch {
    ActorPresentationBatch {
        submissions: Vec::new(),
        skin_layers: Vec::new(),
        artwork: HashMap::new(),
    }
}

/// Runs after normal actor preparation and before its existing frame publication.
#[allow(
    clippy::too_many_arguments,
    reason = "host binding of separate connection and renderer resources"
)]
pub(crate) fn update(
    controller: Option<ResMut<SessionController>>,
    terrain: Option<Res<java_render::Presentation>>,
    presentation: Option<ResMut<Presentation>>,
    mut prepared: ResMut<PreparedActorPublication>,
    mut scene: ResMut<ActorRenderScene>,
    heads: Option<ResMut<super::super::java_items::Heads>>,
    base: Option<Res<ActorArtworkResource>>,
    menu: Option<ResMut<client_shell::menu::MenuRuntime>>,
    time: Res<Time<bevy::time::Real>>,
    clock: Option<Res<super::super::java_cooldowns::Clock>>,
) {
    let (Some(mut controller), Some(terrain), Some(mut presentation)) =
        (controller, terrain, presentation)
    else {
        return;
    };
    let _telemetry = super::super::java_telemetry::scope(
        &controller,
        client_session::java::telemetry::Stage::EntitySkinPublication,
    );
    let p = &mut *presentation;
    let active = controller.java.is_some();
    let state = &mut controller.java_world;
    let scope = active
        .then_some(())
        .and(state.terrain.dimension())
        .map(|d| (d.session, d.epoch));
    if p.scope != scope {
        if p.scope.is_some() {
            p.reset();
            let _ = scene.replace_java_item_geometries(Vec::new());
            if let Some(base) = &base {
                scene.configure_artwork(base.0.clone());
            }
        }
        p.scope = scope;
    }
    if !active {
        return;
    }
    let Some(scope) = scope else {
        prepared.replace_scene(empty(), state.generation(), 0., 0);
        return;
    };
    const MISSING_HEADS: &str = "Native Java item-head resource owner is unavailable";
    let Some(mut heads) = heads else {
        p.failure = Some(MISSING_HEADS.into());
        prepared.replace_scene(empty(), state.generation(), 0., 0);
        return;
    };
    if p.failure.as_deref() == Some(MISSING_HEADS) {
        p.failure = None;
    }
    // Shared Bedrock hosts need no Java clock. An active Java host must still
    // report the missing owner, rather than silently sampling a fabricated tick.
    const MISSING_CLOCK: &str = "Native Java entity simulation clock is unavailable";
    let Some(clock) = clock else {
        p.failure = Some(MISSING_CLOCK.into());
        prepared.replace_scene(empty(), state.generation(), 0., 0);
        return;
    };
    if p.failure.as_deref() == Some(MISSING_CLOCK) {
        p.failure = None;
    }
    p.skins.update(scope, time.elapsed(), &state.entities);
    // The sole world clock also advances during a render stall; sampling never double-ticks.
    p.fraction = clock.fraction();
    p.backlog_ticks = clock.backlog_ticks();
    p.ticks = state.inventory.cooldowns.client_tick();
    let fraction = p.partial_tick();
    p.published = 0;
    p.unsupported = 0;
    p.unknown_light = 0;
    p.glow_pending = 0;
    p.custom_skin_pending = 0;
    p.published_players = 0;
    p.published_villagers = 0;
    p.published_items = 0;
    p.published_pickups = 0;
    p.item_pending = 0;
    if p.assets.is_none() && p.pending.is_none() && p.failure.is_none() {
        let appearances = match state.pig_appearances() {
            Ok(v) => v,
            Err(e) => {
                p.failure = Some(e.into());
                prepared.replace_scene(empty(), scope.0, 0., 0);
                return;
            }
        };
        let root =
            launcher::install_layout::discover_java_assets().map_err(|error| error.to_string());
        let cancel = Arc::clone(&p.cancel);
        let (tx, rx) = crossbeam_channel::bounded(1);
        p.pending = Some(rx);
        if let Err(e) = std::thread::Builder::new()
            .name("java-entity-assets".into())
            .spawn(move || {
                let _ = tx.send(root.and_then(|root| assets::load(root, appearances, cancel)));
            })
        {
            p.pending = None;
            p.failure = Some(e.to_string());
        }
    }
    let arrived = p.pending.as_ref().and_then(|rx| match rx.try_recv() {
        Ok(result) => Some(result),
        Err(crossbeam_channel::TryRecvError::Empty) => None,
        Err(crossbeam_channel::TryRecvError::Disconnected) => {
            Some(Err("Java entity asset worker ended without a result".into()))
        }
    });
    if let Some(result) = arrived {
        p.pending = None;
        match result {
            Ok(assets) => {
                if let Err(e) = scene
                    .insert_geometries(assets.models.values().map(|m| m.geometry.clone()).collect())
                {
                    p.failure = Some(format!("native actor catalog rejected geometry: {e:?}"));
                } else {
                    scene.configure_artwork(assets.artwork.clone());
                    p.assets = Some(assets);
                }
            }
            Err(e) => p.failure = Some(e),
        }
    }
    if let (Some(assets), Some(catalog)) = (&p.assets, terrain.items(scope.0, scope.1)) {
        if p.equipment.as_ref().is_none_or(|e| !e.matches(&catalog)) {
            scene.configure_artwork(assets.artwork.clone());
            let _ = scene.replace_java_item_geometries(Vec::new());
            p.equipment = Some(equipment::State::new(catalog, assets));
        }
        if let Some(equipment) = &mut p.equipment {
            equipment.update(
                state,
                &mut scene,
                &p.skins,
                heads.update(scope, state, time.elapsed()),
            );
        }
    }
    let mut batch = empty();
    let mut skin_layers_full = false;
    if p.failure.is_none()
        && terrain.world_ready() == Some(true)
        && let Some(assets) = &p.assets
    {
        p.serial = match p.serial.checked_add(1) {
            Some(n) => n,
            None => {
                p.failure = Some("Java actor publication exhausted".into());
                prepared.replace_scene(batch, scope.0, 0., 0);
                return;
            }
        };
        let mut retained = BTreeSet::new();
        let mut ground_retained = BTreeSet::new();
        for e in state.entities.iter() {
            if e.kind.as_ref() == java_protocol::target::ENTITY_ITEM {
                ground_retained.insert(e.identity);
                let birth = *p.ground_births.entry(e.identity).or_insert(p.ticks);
                if e.visual.invisible() {
                    continue;
                }
                if e.visual.incomplete.is_some() {
                    p.unsupported += 1;
                    continue;
                }
                let Some(item) = state.equipment.ground(e.identity) else {
                    p.item_pending += 1;
                    continue;
                };
                let Some(stack) = &item.stack else {
                    continue;
                };
                let Some(entry) = p
                    .equipment
                    .as_ref()
                    .and_then(|w| w.ground_entry(e.identity, item.revision))
                else {
                    p.item_pending += 1;
                    continue;
                };
                let position = e
                    .item_motion
                    .as_ref()
                    .filter(|v| v.active)
                    .map_or(e.motion.position, |v| v.sample(fraction));
                let Some(light) = light(&state.terrain, position) else {
                    p.unknown_light += 1;
                    continue;
                };
                if e.visual.glowing() {
                    p.glow_pending += 1;
                }
                match ground::append(
                    &mut batch,
                    ground::Source::from(e),
                    entry,
                    stack.count,
                    ground::Frame {
                        scope,
                        serial: p.serial,
                        age: p.ticks.saturating_sub(birth) as f32 + fraction,
                        position,
                        light,
                    },
                ) {
                    Ok(draws) if draws > 0 => {
                        p.published += 1;
                        p.published_items += 1;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        p.failure = Some(error);
                        break;
                    }
                }
                continue;
            }
            if e.visual.invisible() {
                continue;
            }
            let is_player = e.visual.player.is_some();
            let crouching = is_player && e.visual.pose.as_ref() == "crouching";
            if e.visual.incomplete.is_some()
                || (e.visual.pose.as_ref() != "standing" && !crouching)
                || e.visual.health.is_some_and(|h| h <= 0.)
            {
                p.unsupported += 1;
                continue;
            }
            if e.visual.glowing() {
                p.glow_pending += 1;
            }
            let Some(motion) = &e.presentation else {
                p.unsupported += 1;
                continue;
            };
            let sampled = motion.sample(fraction);
            if let Some(appearance) = &e.visual.villager {
                let Some(light) = light(&state.terrain, sampled.pose.position) else {
                    p.unknown_light += 1;
                    continue;
                };
                retained.insert(e.identity);
                if let Err(error) = villager::append(
                    &mut batch,
                    e,
                    appearance,
                    sampled,
                    assets,
                    &mut p.villagers,
                    ground::Frame {
                        scope,
                        serial: p.serial,
                        age: 0.,
                        light,
                        position: sampled.pose.position,
                    },
                ) {
                    p.failure = Some(error);
                    break;
                }
                p.published_villagers += 1;
                continue;
            }
            let custom_skin = p.skins.get(e.identity).and_then(|(skin, slim)| {
                let layer = skins::frame_layer(&mut batch.skin_layers, skin);
                skin_layers_full |= layer.is_none();
                layer.map(|layer| (layer, slim))
            });
            let (model_name, texture, textures) = if let Some(profile) = &e.visual.player {
                if profile
                    .properties
                    .iter()
                    .any(|p| p.name.as_ref() == "textures")
                {
                    p.custom_skin_pending += usize::from(custom_skin.is_none());
                }
                (
                    if custom_skin
                        .as_ref()
                        .map_or(profile.default_skin.slim, |(_, slim)| *slim)
                    {
                        "player_slim"
                    } else {
                        "player_wide"
                    },
                    profile.default_skin.texture.as_ref(),
                    &assets.player_textures,
                )
            } else if let Some(variant) = &e.visual.pig {
                let name = if e.visual.baby {
                    "baby_pig"
                } else {
                    match variant.model {
                        PigModel::Normal => "pig",
                        PigModel::Cold => "cold_pig",
                    }
                };
                (
                    name,
                    if e.visual.baby {
                        variant.baby_texture.as_ref()
                    } else {
                        variant.adult_texture.as_ref()
                    },
                    &assets.textures,
                )
            } else {
                p.unsupported += 1;
                continue;
            };
            let Some(model) = assets.models.get(model_name) else {
                p.failure = Some(format!("missing native model {model_name}"));
                break;
            };
            let Some(&artwork) = textures.get(texture) else {
                p.failure = Some(format!("unprepared native entity texture {texture}"));
                break;
            };
            let Some(light) = light(&state.terrain, sampled.pose.position) else {
                p.unknown_light += 1;
                continue;
            };
            let gear = state.equipment.get(e.identity);
            let held = gear.map_or([false; 2], |g| [g.slots[0].is_some(), g.slots[1].is_some()]);
            let [right, left] = if e.visual.left_handed {
                [held[1], held[0]]
            } else {
                held
            };
            let posture = is_player.then(|| {
                sampled.player_posture_with_use(
                    crouching,
                    right,
                    left,
                    e.visual.swing.sample(fraction),
                    e.visual.left_handed,
                    client_world::java::equipment::player_use_pose(&e.visual, gear),
                )
            });
            let angles = if let Some(posture) = posture {
                posture.rotations
            } else {
                let r = sampled.rotations();
                [r[0], r[1], r[2], r[3], r[4], [0.; 3]]
            };
            let layers = is_player.then_some(e.visual.player_layers);
            if p.poses.get(&e.identity).is_none_or(|old| {
                !Arc::ptr_eq(&old.model, model) || old.angles != angles || old.layers != layers
            }) {
                let names: &[&str] = if is_player {
                    &world::java::entities::animation::PLAYER_PARTS
                } else {
                    &[
                        "root/head",
                        "root/right_hind_leg",
                        "root/left_hind_leg",
                        "root/right_front_leg",
                        "root/left_front_leg",
                    ]
                };
                let Some(bones) = names
                    .iter()
                    .map(|name| model.bone(name))
                    .collect::<Option<Vec<_>>>()
                else {
                    p.failure = Some("native entity animation bone missing".into());
                    break;
                };
                let offsets = posture.map_or_else(Vec::new, |posture| {
                    bones
                        .iter()
                        .copied()
                        .zip(posture.offsets)
                        .collect::<Vec<_>>()
                });
                let rotations = bones.into_iter().zip(angles).collect::<Vec<_>>();
                let hidden = match hidden_parts(model, layers) {
                    Ok(parts) => parts,
                    Err(error) => {
                        p.failure = Some(error.into());
                        break;
                    }
                };
                match model.pose_with_offsets_and_visibility(&rotations, &offsets, &hidden) {
                    Ok(bones) => {
                        let bounds = match model.posed_bounds(&bones) {
                            Ok(bounds) => bounds,
                            Err(error) => {
                                p.failure = Some(error.to_string());
                                break;
                            }
                        };
                        p.poses.insert(
                            e.identity,
                            Pose {
                                model: Arc::clone(model),
                                angles,
                                layers,
                                bones,
                                bounds,
                            },
                        );
                    }
                    Err(error) => {
                        p.failure = Some(error.to_string());
                        break;
                    }
                }
            }
            retained.insert(e.identity);
            let pose = &p.poses[&e.identity];
            // GPU dimension zero means the current render slot. Session, epoch and incarnation
            // bind the native identity; no Java identifier is interpreted as a Bedrock dimension.
            let Some(spawn_revision) = std::num::NonZeroU64::new(e.identity.incarnation) else {
                p.failure = Some("Java entity has no admitted incarnation".into());
                break;
            };
            let identity = ActorRenderIdentity {
                session_id: scope.0,
                dimension: 0,
                runtime_id: e.identity.incarnation,
                spawn_revision,
                ingress_sequence: e.revision,
                source_tick: None,
                movement_revision: e.revision,
                pose_generation: render_api::PoseGeneration::new(p.serial),
                layer: render_model::ACTOR_LAYER_BODY,
            };
            let matrix = Mat4::from_scale_rotation_translation(
                Vec3::splat(if is_player { 0.9375 } else { 1. }),
                Quat::from_rotation_y(-sampled.body_yaw.to_radians()),
                Vec3::from_array(sampled.pose.position.map(|v| v as f32))
                    + Vec3::Y * posture.map_or(0., |p| p.render_offset_y),
            )
            .to_cols_array_2d();
            let runtime_layer = custom_skin.map(|(layer, _)| layer);
            batch.submissions.push(ActorRigSubmission {
                material: ActorMaterial::default(),
                culling_bounds: pose.bounds,
                input: ActorRigRenderInput {
                    identity,
                    rig: model.geometry.id,
                    previous_bones: Arc::clone(&pose.bones),
                    current_bones: Arc::clone(&pose.bones),
                    completed_tick: p.serial,
                    reset_generation: scope.1,
                },
                world_from_actor: std::array::from_fn(|r| {
                    [matrix[0][r], matrix[1][r], matrix[2][r], matrix[3][r]]
                }),
                texture_layer: runtime_layer.unwrap_or(artwork.layer),
                route: ActorRigRoute::Compiled,
                tint: 0,
                overlay_rgba8: if e.visual.hurt_ticks != 0 {
                    render_model::pack_overlay_rgba8([1., 0., 0., 77. / 255.])
                } else {
                    0
                },
                uv_anim: [0., 0., 1., 1.],
                light,
            });
            if let (Some(posture), Some(gear), Some(equipment)) = (posture, gear, &p.equipment)
                && let Some(entry) = equipment.entry(e, gear, &p.skins)
            {
                let body = batch.submissions.last().unwrap().clone();
                if let Err(error) = equipment::append(
                    &mut batch,
                    entry,
                    gear,
                    &e.visual,
                    &body,
                    model,
                    posture,
                    p.ticks as f32 / 20. + fraction / 20.,
                ) {
                    p.failure = Some(error);
                    break;
                }
            }
            if runtime_layer.is_none() {
                batch.artwork.insert(identity, artwork);
            }
            p.published_players += usize::from(is_player);
        }
        for pickup in state.equipment.pickups() {
            ground_retained.insert(pickup.source);
            let Some(stack) = pickup.ground.stack.as_ref() else {
                continue;
            };
            let Some(entry) = p.equipment.as_ref().and_then(|work| {
                work.ground_entry(pickup.identity, pickup.ground.revision)
                    .or_else(|| work.ground_entry(pickup.source, pickup.ground.revision))
            }) else {
                p.item_pending += 1;
                continue;
            };
            let position = pickup.flight.sample(fraction);
            let Some(light) = light(&state.terrain, pickup.flight.origin()) else {
                p.unknown_light += 1;
                continue;
            };
            match ground::append(
                &mut batch,
                ground::Source {
                    identity: pickup.identity,
                    uuid: pickup.uuid,
                    revision: pickup.revision,
                },
                entry,
                stack.count,
                ground::Frame {
                    scope,
                    serial: p.serial,
                    age: pickup.age,
                    position,
                    light,
                },
            ) {
                Ok(draws) if draws > 0 => {
                    p.published += 1;
                    p.published_pickups += 1;
                }
                Ok(_) => {}
                Err(error) => {
                    p.failure = Some(error);
                    break;
                }
            }
        }
        p.poses.retain(|id, _| retained.contains(id));
        p.villagers.retain(|(id, _), _| retained.contains(id));
        p.ground_births.retain(|id, _| ground_retained.contains(id));
    }
    if let Some(error) = &p.failure {
        batch = empty();
        p.published_players = 0;
        p.published_villagers = 0;
        p.poses.clear();
        p.villagers.clear();
        if let Some(mut menu) = menu {
            menu.set_java_diagnostic(format!("Java entities incomplete: {error}. Check the pinned models.json and client.jar in the packaged Java asset directory."));
        }
    }
    if skin_layers_full && !p.skin_layers_full {
        bevy::log::warn!(target:"java_session",limit=render_model::MAX_RENDERED_PLAYERS,"Native Java skin frame capacity reached; excess players retain default skins");
    }
    p.skin_layers_full = skin_layers_full;
    p.published = batch.submissions.len();
    let counts = [
        state.entities.iter().count(),
        p.published_players,
        p.unsupported,
        p.unknown_light,
        p.custom_skin_pending,
    ];
    if p.ticks >= p.log_after_tick && p.logged_counts != Some(counts) {
        bevy::log::debug!(target: "java_session", received_entities = counts[0],
            rendered_players = counts[1], unsupported_entities = counts[2],
            awaiting_light = counts[3], custom_skins_pending = counts[4],
            assets_preparing = p.pending.is_some(), error = ?p.failure,
            "Native Java entity presentation status");
        p.logged_counts = Some(counts);
        p.log_after_tick = p
            .ticks
            .saturating_add(u64::from(simulation_clock::TICKS_PER_SECOND));
    }
    prepared.replace_scene(batch, scope.0, 0., p.unsupported + p.unknown_light);
}

/// Current slice samples authoritative light at the feet. Eye-height probing remains unqualified.
fn light(terrain: &world::java::Terrain, position: [f64; 3]) -> Option<u32> {
    let [x, y, z] = position.map(|n| n.floor() as i32);
    let [block, sky] = terrain.light([x, y, z])?;
    Some(render_model::pack_actor_light(block, sky))
}

/// Native outer-layer switches bind to named parts in the verified player hierarchy.
fn hidden_parts(
    model: &render_model::actor::java_geometry::Model,
    layers: Option<world::java::entities::visual::PlayerLayers>,
) -> Result<Vec<usize>, &'static str> {
    let Some(layers) = layers else {
        return Ok(Vec::new());
    };
    [
        ("root/head/hat", layers.hat),
        ("root/body/jacket", layers.jacket),
        ("root/left_arm/left_sleeve", layers.left_sleeve),
        ("root/right_arm/right_sleeve", layers.right_sleeve),
        ("root/left_leg/left_pants", layers.left_pants),
        ("root/right_leg/right_pants", layers.right_pants),
    ]
    .into_iter()
    .filter(|(_, visible)| !visible)
    .map(|(name, _)| {
        model
            .bone(name)
            .ok_or("native player clothing bone missing")
    })
    .collect()
}
