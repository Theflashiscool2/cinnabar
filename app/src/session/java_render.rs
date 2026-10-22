//! Native Java world presentation through Cinnabar's existing chunk renderer.
mod assets;
use super::SessionController;
use bevy::prelude::*;
use client_presentation::local_player::LocalViewPose;
use client_shell::menu::MenuRuntime;
use render_api::{PublicationAllowance, PublicationServiceConfig};
use render_scene::{
    ChunkBiomeTints, ChunkRenderQueue, ChunkTextureAssets, ChunkUploadAcknowledgements,
    ChunkUploadPriority, ChunkUploadToken,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use world::java::Dimension;

#[derive(Resource)]
pub(crate) struct Presentation {
    dimension: Option<Dimension>,
    pending: Option<crossbeam_channel::Receiver<Result<assets::Prepared, String>>>,
    cancel: Arc<AtomicBool>,
    stream: Option<chunk_pipeline::java::Stream>,
    unsupported_tints: BTreeSet<u32>,
    failure: Option<String>,
    published: BTreeMap<[i32; 3], (u64, Instant, bool)>,
    removals: BTreeSet<[i32; 3]>,
    allowance: PublicationAllowance,
    frame: u64,
    teleport: Option<u64>,
    opened: bool,
    next_generation: u64,
    uploaded: BTreeSet<[i32; 3]>,
}
impl Default for Presentation {
    fn default() -> Self {
        Self {
            dimension: None,
            pending: None,
            cancel: Arc::new(AtomicBool::new(false)),
            stream: None,
            unsupported_tints: BTreeSet::new(),
            failure: None,
            published: BTreeMap::new(),
            removals: BTreeSet::new(),
            allowance: PublicationAllowance::new(PublicationServiceConfig::DEFAULT),
            frame: 0,
            teleport: None,
            opened: false,
            next_generation: 0,
            uploaded: BTreeSet::new(),
        }
    }
}
impl Drop for Presentation {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Presentation {
    /// Native readiness owns Java world entry; absence leaves the Bedrock gate in charge.
    pub(crate) fn world_ready(&self) -> Option<bool> {
        self.dimension.as_ref().map(|_| self.opened)
    }

    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({
            "session": self.dimension.as_ref().map(|d| d.session),
            "epoch": self.dimension.as_ref().map(|d| d.epoch),
            "spawn_terrain_uploaded": self.opened,
            "submitted_sections": self.published.len(),
            "uploaded_nonempty_sections": self.uploaded.len(),
            "backlog": self.stream.as_ref().map(|s| s.backlog()),
            "workers": self.stream.as_ref().map(|s| serde_json::json!({
                "dispatched": s.metrics.dispatched,
                "completed": s.metrics.completed,
                "obsolete": s.metrics.obsolete,
                "waiting_light": s.metrics.waiting_light,
            })),
            "error": self.failure,
            "movement_supported": false,
            "playable": false,
        })
    }

    fn reset(&mut self) {
        self.cancel.store(true, Ordering::Release);
        let generation = self.next_generation;
        let old = std::mem::take(self);
        self.next_generation = generation;
        client_runtime::session::release_off_frame(old);
    }
}
#[allow(
    clippy::too_many_arguments,
    reason = "the host binds separate session, view and shared render owners"
)]
pub(crate) fn update(
    mut unbound: Option<ResMut<client_interaction::camera::UnboundCameraMovement>>,
    controller: Option<ResMut<SessionController>>,
    mut presentation: ResMut<Presentation>,
    menu: Option<ResMut<MenuRuntime>>,
    view: Option<ResMut<LocalViewPose>>,
    queue: Option<ResMut<ChunkRenderQueue>>,
    textures: Option<ResMut<ChunkTextureAssets>>,
    tints: Option<ResMut<ChunkBiomeTints>>,
    acknowledgements: Option<Res<ChunkUploadAcknowledgements>>,
) {
    let (
        Some(mut controller),
        Some(mut menu),
        Some(mut view),
        Some(mut queue),
        Some(mut textures),
        Some(mut tints),
        Some(acknowledgements),
    ) = (
        controller,
        menu,
        view,
        queue,
        textures,
        tints,
        acknowledgements,
    )
    else {
        return;
    };
    if let Some(policy) = unbound.as_mut() {
        policy.0 = controller.java.is_none();
    }
    let p = &mut *presentation;
    let active = controller
        .java
        .as_ref()
        .and(controller.java_world.terrain.dimension())
        .cloned();
    if p.dimension != active {
        if p.dimension.is_some() {
            queue.reset_session();
            acknowledgements.clear();
        }
        p.reset();
        p.dimension = active;
    }
    let Some(dimension) = p.dimension.clone() else {
        return;
    };
    let state = &mut controller.java_world;
    if let Some(position) = &state.teleport
        && p.teleport != Some(position.revision)
    {
        let feet = Vec3::from_array(position.motion.position.map(|n| n as f32));
        // Standing Java eye height; pose changes belong to the upcoming native movement slice.
        view.set_subject_position(feet + Vec3::Y * 1.62, feet);
        view.set_rotation(
            Quat::from_rotation_y((-position.motion.yaw + 180.).to_radians())
                * Quat::from_rotation_x(-position.motion.pitch.to_radians()),
        );
        view.reanchor_camera();
        p.teleport = Some(position.revision);
    }
    if p.failure.is_none()
        && p.stream.is_none()
        && p.pending.is_none()
        && initial_halo_ready(&state.terrain)
        && state.teleport.is_some()
    {
        let root = std::env::var_os("CINNABAR_JAVA_ASSETS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(".local/assets/java")
                    .join(java_protocol::target::GAME_VERSION)
            });
        let columns = state
            .terrain
            .columns()
            .map(|(_, c)| Arc::clone(c))
            .collect();
        let biomes = state.biomes.clone();
        let cancel = Arc::clone(&p.cancel);
        let (sender, receiver) = crossbeam_channel::bounded(1);
        p.pending = Some(receiver);
        std::thread::Builder::new()
            .name("java-assets".into())
            .spawn(move || {
                let _ = sender.send(assets::load(root, columns, biomes, cancel));
            })
            .expect("Java asset worker");
    }
    if let Some(receiver) = &p.pending
        && let Ok(result) = receiver.try_recv()
    {
        p.pending = None;
        match result {
            Ok(prepared) => {
                let revision = textures.revision.saturating_add(1);
                *textures = ChunkTextureAssets::with_revision(
                    Arc::clone(&prepared.graphics.runtime),
                    revision,
                );
                *tints = ChunkBiomeTints::with_identity(
                    Arc::from(prepared.tints),
                    meshing::ChunkBiomeTintIdentity::new(dimension.session, dimension.epoch),
                );
                p.unsupported_tints = prepared.unsupported_tints;
                p.stream = chunk_pipeline::java::Stream::new(&state.terrain, prepared.graphics);
            }
            Err(error) => {
                p.failure = Some(format!(
                    "Java runtime assets: {error}. Supply the pinned client.jar and blocks.json under CINNABAR_JAVA_ASSETS."
                ))
            }
        }
    }
    p.frame += 1;
    let config = PublicationServiceConfig::DEFAULT;
    p.allowance.begin_frame(
        p.frame,
        config.maximum_frame_items,
        config.maximum_frame_bytes,
        config.maximum_zero_byte_operations_per_frame,
        config.maximum_frame_items,
    );
    if let Some(stream) = p.stream.as_mut() {
        for chunk in stream.poll(&mut state.terrain) {
            for &section in p.published.keys().filter(|s| [s[0], s[2]] == chunk) {
                p.removals.insert(section);
            }
        }
        while let Some(ready) = stream.pop_ready(&state.terrain) {
            if ready
                .biome
                .uniform_tint_index()
                .is_some_and(|id| p.unsupported_tints.contains(&id))
            {
                p.failure=Some("Java grass color modifiers need native spatial tinting; this world is not fully rendered".into());
                continue;
            }
            let bytes = ChunkRenderQueue::upload_byte_len(&ready.mesh, &ready.biome);
            let Some(permit) = terrain_permit(&p.allowance, bytes) else {
                stream.retry_front(ready);
                break;
            };
            // The renderer sees one active world slot. Session/epoch plus revision fence its reuse;
            // this zero is not a Java or Bedrock network dimension ID.
            let [x, y, z] = ready.section;
            let key = world::SubChunkKey::new(0, x, y, z);
            p.next_generation += 1;
            let token = ChunkUploadToken {
                generation: p.next_generation,
                dirty_since: Instant::now(),
            };
            let nonempty = !ready.mesh.model_draw_refs().is_empty()
                || !ready.mesh.transparent_model_draw_refs().is_empty();
            match queue.try_update_tracked_with_biome_identity_permitted(
                key,
                ready.mesh,
                ready.biome,
                tints.table_identity(),
                ChunkUploadPriority::from_camera(key, view.eye_translation()),
                token,
                permit,
            ) {
                Ok(()) => {
                    p.uploaded.remove(&ready.section);
                    p.published.insert(
                        ready.section,
                        (token.generation, token.dirty_since, nonempty),
                    );
                }
                Err((mesh, biome, _permit)) => {
                    stream.retry_front(chunk_pipeline::java::Ready {
                        key: ready.key,
                        section: ready.section,
                        mesh,
                        biome,
                    });
                    break;
                }
            }
        }
        if let Some(error) = stream.errors().values().next() {
            p.failure = Some(error.clone());
        }
    }
    while let Some(section) = p.removals.pop_first() {
        let Some(permit) = p.allowance.try_admit_zero_byte() else {
            p.removals.insert(section);
            break;
        };
        let [x, y, z] = section;
        let key = world::SubChunkKey::new(0, x, y, z);
        p.next_generation += 1;
        let generation = p.next_generation;
        let token = ChunkUploadToken {
            generation,
            dirty_since: Instant::now(),
        };
        match queue.try_remove_tracked_permitted(
            key,
            ChunkUploadPriority::urgent(),
            token,
            render_api::TerrainRemovalKind::Withdrawn,
            permit,
        ) {
            Ok(()) => {
                p.published.remove(&section);
                p.uploaded.remove(&section);
            }
            Err(_) => {
                p.removals.insert(section);
                break;
            }
        }
    }
    for ack in acknowledgements.drain() {
        let section = [ack.key.x, ack.key.y, ack.key.z];
        if p.published
            .get(&section)
            .is_some_and(|(generation, since, nonempty)| {
                *generation == ack.token.generation && *since == ack.token.dirty_since && *nonempty
            })
        {
            p.uploaded.insert(section);
        }
    }
    let spawn_ready = state.teleport.as_ref().is_some_and(|t| {
        let section = t.motion.position.map(|v| (v.floor() as i32).div_euclid(16));
        p.uploaded.contains(&section)
    });
    if let Some(error) = &p.failure {
        if p.opened {
            menu.show_connecting();
            controller.connecting = true;
            p.opened = false;
        }
        menu.set_java_diagnostic(format!("Java terrain incomplete: {error}"));
    } else if !p.opened && spawn_ready && !state.awaiting_correction_ack() {
        // Actual nonempty spawn terrain reached GPU upload; this does not claim gameplay support.
        if let Some(java) = &controller.java
            && java
                .command(java_protocol::session::Command::PlayerLoaded {
                    dimension_epoch: dimension.epoch,
                })
                .is_err()
        {
            return;
        }
        p.opened = true;
        controller.connecting = false;
        menu.show_world();
        bevy::log::info!(
            session = dimension.session,
            epoch = dimension.epoch,
            "Java spawn terrain uploaded by Cinnabar renderer; movement and gameplay remain unsupported"
        );
    }
}

fn initial_halo_ready(terrain: &world::java::Terrain) -> bool {
    let Some([x, z]) = terrain.center else {
        return false;
    };
    (-1..=1).all(|dx| (-1..=1).all(|dz| terrain.column([x + dx, z + dz]).is_some()))
}

/// Empty geometry still publishes authority, using the renderer's zero-byte budget.
fn terrain_permit(
    allowance: &PublicationAllowance,
    bytes: u64,
) -> Option<render_api::PublicationPermit> {
    if bytes == 0 {
        allowance.try_admit_zero_byte()
    } else {
        allowance.try_admit_payload(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_section_does_not_block_following_terrain() {
        let allowance = PublicationAllowance::new(PublicationServiceConfig::DEFAULT);
        allowance.begin_frame(1, 2, 64, 1, 2);
        let empty = terrain_permit(&allowance, 0).expect("empty authority must advance");
        assert!(empty.is_zero_byte());
        let terrain =
            terrain_permit(&allowance, 64).expect("visible terrain follows empty section");
        assert_eq!(terrain.bytes(), Some(64));
        assert!(!terrain.is_zero_byte());
        assert!(terrain_permit(&allowance, 1).is_none());
    }
}
