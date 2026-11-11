//! Native entity presentation through the shared actor GPU publication boundary.
mod assets;
mod equipment;
mod ground;
mod skins;
mod villager;
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
    angles: [[f32; 3]; 6],
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
    equipment: Option<equipment::State>,
    poses: BTreeMap<Identity, Pose>,
    villagers: BTreeMap<(Identity, bool), villager::Pose>,
    ground_births: BTreeMap<Identity, u64>,
    published_items: usize,
    item_pending: usize,
    serial: u64,
    failure: Option<String>,
    published: usize,
    unsupported: usize,
    unknown_light: usize,
    glow_pending: usize,
    custom_skin_pending: usize,
    skin_layers_full: bool,
    skins: skins::Players,
    published_players: usize,
    published_villagers: usize,
    log_after_tick: u64,
    logged_counts: Option<[usize; 5]>,
    fraction: f32,
    backlog_ticks: u128,
    ticks: u64,
}
impl Presentation {
    pub(super) fn telemetry_progress(&self) -> serde_json::Value {
        serde_json::json!({"preparing":self.pending.is_some(),"players":self.published_players,
            "villagers":self.published_villagers,"items":self.published_items,
            "unsupported":self.unsupported,"awaiting_light":self.unknown_light,
            "skins_pending":self.custom_skin_pending,"skin_capacity_exhausted":self.skin_layers_full,
            "items_pending":self.item_pending,"error_present":self.failure.is_some()})
    }
    pub(super) fn telemetry_loading(&self) -> bool {
        self.pending.is_some() || self.custom_skin_pending > 0 || self.item_pending > 0
    }
}
impl Default for Presentation {
    fn default() -> Self {
        Self {
            scope: None,
            cancel: Arc::new(AtomicBool::new(false)),
            pending: None,
            assets: None,
            equipment: None,
            poses: BTreeMap::new(),
            villagers: BTreeMap::new(),
            ground_births: BTreeMap::new(),
            published_items: 0,
            item_pending: 0,
            serial: 0,
            failure: None,
            published: 0,
            unsupported: 0,
            unknown_light: 0,
            glow_pending: 0,
            custom_skin_pending: 0,
            skin_layers_full: false,
            skins: Default::default(),
            published_players: 0,
            published_villagers: 0,
            log_after_tick: 0,
            logged_counts: None,
            fraction: 0.,
            backlog_ticks: 0,
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
    pub(super) fn first_person_arm(
        &self,
    ) -> Option<&Arc<render_model::actor::java_geometry::Model>> {
        self.assets.as_ref().map(|assets| &assets.arm)
    }

    /// Shared sampling fraction for this native actor frame, including its world-space labels.
    pub(crate) fn partial_tick(&self) -> f32 {
        self.fraction
    }
    fn reset(&mut self) {
        self.cancel.store(true, Ordering::Release);
        let serial = self.serial;
        let mut skins = std::mem::take(&mut self.skins);
        skins.retire();
        let old = std::mem::take(self);
        self.skins = skins;
        self.serial = serial;
        client_runtime::session::release_off_frame(old);
    }
    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({"scope":self.scope,"preparing":self.pending.is_some(),"published":self.published,"unsupported":self.unsupported,"unknown_light":self.unknown_light,"glow_pending":self.glow_pending,"retained_poses":self.poses.len()+self.villagers.len(),"error":self.failure,"presentation":"native_living_interpolation","published_players":self.published_players,"published_villagers":self.published_villagers,"published_items":self.published_items,"item_pending":self.item_pending,"item_motion":"native_dry_ticks_or_explicit_server_fallback","custom_skin_pending":self.custom_skin_pending,"skin_frame_capacity_exhausted":self.skin_layers_full,"skins":self.skins.diagnostic(),"interpolation":true,"gait_animation":true,"ticks":self.ticks,"backlog_ticks":self.backlog_ticks,"light_probe":"feet","equipment":self.equipment.as_ref().map(|e|e.diagnostic()),"visual_parity":false})
    }
}
pub(crate) mod publish;

/// Only the active edition may replace the shared actor catalog and prepared scene.
pub(crate) fn uses_bedrock_actor_authority(session: Option<Res<SessionController>>) -> bool {
    session.is_none_or(|session| session.java.is_none())
}
