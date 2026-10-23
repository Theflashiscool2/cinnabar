//! Native entity presentation through the shared actor GPU publication boundary.
mod assets;
use super::{SessionController, java_render};
use bevy::prelude::*;
use client_presentation::{
    actor_publication::PreparedActorPublication, presentation::actors::ActorPresentationBatch,
};
use render_model::{
    ActorMaterial, ActorRenderIdentity, ActorRigRenderInput, ActorRigRoute, ActorRigSubmission,
    RenderBoneTransform,
};
use render_scene::{ActorArtworkResource, ActorRenderScene};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use world::java::entities::{Identity, visual::PigModel};

struct Pose {
    model: Arc<render_model::actor::java_geometry::Model>,
    angles: [[f32; 3]; 5],
    layers: Option<world::java::entities::visual::PlayerLayers>,
    bones: Arc<[RenderBoneTransform]>,
    bounds: ::assets::SkinGeometryBounds,
}
#[derive(Resource)]
pub(crate) struct Presentation {
    scope: Option<(u64, u64)>,
    cancel: Arc<AtomicBool>,
    pending: Option<crossbeam_channel::Receiver<Result<assets::Prepared, String>>>,
    assets: Option<assets::Prepared>,
    poses: BTreeMap<Identity, Pose>,
    serial: u64,
    failure: Option<String>,
    published: usize,
    unsupported: usize,
    unknown_light: usize,
    glow_pending: usize,
    custom_skin_pending: usize,
    published_players: usize,
    elapsed: std::time::Duration,
    ticks: u64,
}
impl Default for Presentation {
    fn default() -> Self {
        Self {
            scope: None,
            cancel: Arc::new(AtomicBool::new(false)),
            pending: None,
            assets: None,
            poses: BTreeMap::new(),
            serial: 0,
            failure: None,
            published: 0,
            unsupported: 0,
            unknown_light: 0,
            glow_pending: 0,
            custom_skin_pending: 0,
            published_players: 0,
            elapsed: std::time::Duration::ZERO,
            ticks: 0,
        }
    }
}
impl Drop for Presentation {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Presentation {
    fn reset(&mut self) {
        self.cancel.store(true, Ordering::Release);
        let serial = self.serial;
        let old = std::mem::take(self);
        self.serial = serial;
        client_runtime::session::release_off_frame(old);
    }
    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({"scope":self.scope,"preparing":self.pending.is_some(),"published":self.published,"unsupported":self.unsupported,"unknown_light":self.unknown_light,"glow_pending":self.glow_pending,"retained_poses":self.poses.len(),"error":self.failure,"presentation":"native_pig_and_player_interpolation","published_players":self.published_players,"custom_skin_pending":self.custom_skin_pending,"interpolation":true,"gait_animation":true,"ticks":self.ticks,"backlog_ticks":self.elapsed.as_nanos()/simulation_clock::TICK_DURATION.as_nanos(),"light_probe":"feet","visual_parity":false})
    }
}
pub(crate) mod publish;

/// Only the active edition may replace the shared actor catalog and prepared scene.
pub(crate) fn uses_bedrock_actor_authority(session: Option<Res<SessionController>>) -> bool {
    session.is_none_or(|session| session.java.is_none())
}
