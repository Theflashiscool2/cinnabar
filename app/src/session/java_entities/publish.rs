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
    controller: Option<Res<SessionController>>,
    terrain: Option<Res<java_render::Presentation>>,
    presentation: Option<ResMut<Presentation>>,
    mut prepared: ResMut<PreparedActorPublication>,
    mut scene: ResMut<ActorRenderScene>,
    base: Option<Res<ActorArtworkResource>>,
    menu: Option<ResMut<client_shell::menu::MenuRuntime>>,
) {
    let (Some(controller), Some(terrain), Some(mut presentation)) =
        (controller, terrain, presentation)
    else {
        return;
    };
    let p = &mut *presentation;
    let state = &controller.java_world;
    let scope = controller
        .java
        .as_ref()
        .and(state.terrain.dimension())
        .map(|d| (d.session, d.epoch));
    if p.scope != scope {
        if p.scope.is_some() {
            p.reset();
            if let Some(base) = &base {
                scene.configure_artwork(base.0.clone());
            }
        }
        p.scope = scope;
    }
    if controller.java.is_none() {
        return;
    }
    let Some(scope) = scope else {
        prepared.replace_scene(empty(), state.generation(), 0., 0);
        return;
    };
    p.published = 0;
    p.unsupported = 0;
    p.unknown_light = 0;
    p.glow_pending = 0;
    if p.assets.is_none() && p.pending.is_none() && p.failure.is_none() {
        let appearances = match state.pig_appearances() {
            Ok(v) => v,
            Err(e) => {
                p.failure = Some(e.into());
                prepared.replace_scene(empty(), scope.0, 0., 0);
                return;
            }
        };
        let root = std::env::var_os("CINNABAR_JAVA_ASSETS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(".local/assets/java")
                    .join(java_protocol::target::GAME_VERSION)
            });
        let cancel = Arc::clone(&p.cancel);
        let (tx, rx) = crossbeam_channel::bounded(1);
        p.pending = Some(rx);
        if let Err(e) = std::thread::Builder::new()
            .name("java-entity-assets".into())
            .spawn(move || {
                let _ = tx.send(assets::load(root, appearances, cancel));
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
    let mut batch = empty();
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
        for e in state.entities.iter() {
            if e.visual.invisible() {
                continue;
            }
            let Some(variant) = &e.visual.pig else {
                p.unsupported += 1;
                continue;
            };
            if e.visual.incomplete.is_some()
                || e.visual.pose.as_ref() != "standing"
                || e.visual.health.is_some_and(|h| h <= 0.)
            {
                p.unsupported += 1;
                continue;
            }
            if e.visual.glowing() {
                p.glow_pending += 1;
            }
            let model_name = if e.visual.baby {
                "baby_pig"
            } else {
                match variant.model {
                    PigModel::Normal => "pig",
                    PigModel::Cold => "cold_pig",
                }
            };
            let Some(model) = assets.models.get(model_name) else {
                p.failure = Some(format!("missing native model {model_name}"));
                break;
            };
            let texture = if e.visual.baby {
                &variant.baby_texture
            } else {
                &variant.adult_texture
            };
            let Some(&artwork) = assets.textures.get(texture.as_ref()) else {
                p.failure = Some(format!("unprepared native entity texture {texture}"));
                break;
            };
            let Some(light) = light(&state.terrain, e.motion.position) else {
                p.unknown_light += 1;
                continue;
            };
            let angles = [
                e.motion.pitch.to_radians(),
                ((e.head_yaw - e.motion.yaw + 180.).rem_euclid(360.) - 180.).to_radians(),
            ];
            if p.poses
                .get(&e.identity)
                .is_none_or(|old| !Arc::ptr_eq(&old.model, model) || old.angles != angles)
            {
                let Some(head) = model.bone("root/head") else {
                    p.failure = Some("native pig head bone missing".into());
                    break;
                };
                match model.pose(&[(head, [angles[0], angles[1], 0.])]) {
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
            let identity = ActorRenderIdentity {
                session_id: scope.0,
                dimension: 0,
                runtime_id: e.identity.incarnation,
                spawn_revision: e.identity.incarnation,
                ingress_sequence: e.revision,
                source_tick: None,
                movement_revision: e.revision,
                pose_generation: render_api::PoseGeneration::new(p.serial),
                layer: render_model::ACTOR_LAYER_BODY,
            };
            let matrix = Mat4::from_rotation_translation(
                Quat::from_rotation_y(-e.motion.yaw.to_radians()),
                Vec3::from_array(e.motion.position.map(|v| v as f32)),
            )
            .to_cols_array_2d();
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
                texture_layer: artwork.layer,
                route: ActorRigRoute::Compiled,
                tint: 0,
                overlay_rgba8: 0,
                uv_anim: [0., 0., 1., 1.],
                light,
            });
            batch.artwork.insert(identity, artwork);
        }
        p.poses.retain(|id, _| retained.contains(id));
    }
    if let Some(error) = &p.failure {
        batch = empty();
        p.poses.clear();
        if let Some(mut menu) = menu {
            menu.set_java_diagnostic(format!("Java entities incomplete: {error}. Check the pinned models.json and client.jar in CINNABAR_JAVA_ASSETS."));
        }
    }
    p.published = batch.submissions.len();
    prepared.replace_scene(batch, scope.0, 0., p.unsupported + p.unknown_light);
}

/// Current slice samples authoritative light at the feet. Eye-height probing remains unqualified.
fn light(terrain: &world::java::Terrain, position: [f64; 3]) -> Option<u32> {
    let [x, y, z] = position.map(|n| n.floor() as i32);
    let dimension = terrain.dimension()?;
    let column = terrain.column([x.div_euclid(16), z.div_euclid(16)])?;
    let section = usize::try_from(y.div_euclid(16) - dimension.min_y.div_euclid(16) + 1).ok()?;
    let index = (y.rem_euclid(16) * 256 + z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
    let read = |layers: &[Option<Arc<[u8]>>]| {
        let bytes = layers.get(section)?.as_ref()?;
        if bytes.is_empty() {
            Some(0)
        } else {
            bytes.get(index / 2).map(|v| (v >> ((index & 1) * 4)) & 15)
        }
    };
    Some(render_model::pack_actor_light(
        read(&column.light.block)?,
        read(&column.light.sky)?,
    ))
}
