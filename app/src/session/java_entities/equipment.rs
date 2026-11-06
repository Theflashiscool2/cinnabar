//! Change-driven equipment artwork. One replaceable worker; world/inventory remain authoritative.
use super::assets;
use ::assets::registry::java::items::{Catalog, Stack};
use render_scene::ActorRenderScene;
use std::sync::{Condvar, Mutex};
use std::{collections::BTreeMap, sync::Arc};
use world::java::entities::Identity;
mod draw;
mod work;
pub(super) use draw::append;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Signature {
    revision: u64,
    slim: bool,
    left: bool,
}
pub(super) struct Player {
    id: Identity,
    signature: Signature,
    slots: [inventory::java::Slot; 8],
}
pub(super) struct Ground {
    id: Identity,
    revision: u64,
    stack: inventory::java::Stack,
}
pub(super) struct Draw {
    pub model: Option<Arc<render_model::actor::java_geometry::Model>>,
    pub rig: render_model::EntityRigId,
    pub hand: Option<bool>,
    pub bounds: ::assets::SkinGeometryBounds,
    pub artwork: render_model::ActorArtworkLocation,
    pub alpha: render_model::HandItemAlphaMode,
    pub glint: bool,
}
pub(super) struct Entry {
    signature: Signature,
    pub ground: bool,
    pub seed: i64,
    pub draws: Vec<Draw>,
}
pub(super) struct Output {
    pub entries: BTreeMap<Identity, Entry>,
    pub artwork: render_model::ActorArtworkPages,
    pub geometries: Vec<render_model::ActorRigGeometry>,
    pub errors: Vec<String>,
}
struct Request {
    revision: u64,
    players: Vec<Player>,
    grounds: Vec<Ground>,
}
#[derive(Default)]
struct Shared {
    closed: bool,
    desired: Option<u64>,
    request: Option<Request>,
    result: Option<(u64, Result<Output, String>)>,
}
pub(super) struct State {
    catalog: Arc<Catalog>,
    shared: Arc<(Mutex<Shared>, Condvar)>,
    installed: Option<Output>,
    error: Option<String>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.shared.0.lock().unwrap().closed = true;
        self.shared.1.notify_one();
    }
}
fn signature(
    e: &world::java::entities::Entity,
    gear: &client_world::java::equipment::Equipment,
) -> Signature {
    Signature {
        revision: gear.revision,
        slim: e
            .visual
            .player
            .as_ref()
            .is_some_and(|p| p.default_skin.slim),
        left: e.visual.left_handed,
    }
}
impl State {
    pub fn new(catalog: Arc<Catalog>, assets: &assets::Prepared) -> Self {
        let shared = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let source = catalog.clone();
        let thread = shared.clone();
        let models = assets.models.clone();
        let base = assets.artwork.clone();
        let root = launcher::install_layout::discover_java_assets().map_err(|e| e.to_string());
        let spawn = std::thread::Builder::new()
            .name("java-world-equipment".into())
            .spawn(move || work::run(source, thread, models, base, root));
        Self {
            catalog,
            shared,
            installed: None,
            error: spawn.err().map(|e| e.to_string()),
        }
    }
    pub fn matches(&self, catalog: &Arc<Catalog>) -> bool {
        Arc::ptr_eq(&self.catalog, catalog)
    }
    pub fn update(&mut self, state: &client_world::java::WorldState, scene: &mut ActorRenderScene) {
        if self.error.is_some() {
            return;
        }
        let mut work = self.shared.0.lock().unwrap();
        let revision = state.equipment.revision();
        if work.desired != Some(revision) {
            let players = state
                .entities
                .iter()
                .filter(|e| e.visual.player.is_some())
                .filter_map(|e| {
                    let gear = state.equipment.get(e.identity)?;
                    Some(Player {
                        id: e.identity,
                        signature: signature(e, gear),
                        slots: gear.slots.clone(),
                    })
                })
                .collect();
            let grounds = state
                .entities
                .iter()
                .filter(|e| e.kind.as_ref() == java_protocol::target::ENTITY_ITEM)
                .filter_map(|e| {
                    let ground = state.equipment.ground(e.identity)?;
                    Some(Ground {
                        id: e.identity,
                        revision: ground.revision,
                        stack: ground.stack.clone()?,
                    })
                })
                .collect();
            work.desired = Some(revision);
            work.request = Some(Request {
                revision,
                players,
                grounds,
            });
            work.result = None;
            self.shared.1.notify_one();
        }
        if let Some((ticket, result)) = work.result.take()
            && work.desired == Some(ticket)
        {
            match result {
                Ok(output) => {
                    if let Err(e) = scene.replace_java_item_geometries(output.geometries.clone()) {
                        self.error = Some(format!("native equipment geometry: {e:?}"));
                        return;
                    }
                    if !output.errors.is_empty()
                        && self
                            .installed
                            .as_ref()
                            .is_none_or(|old| old.errors != output.errors)
                    {
                        bevy::log::warn!(target:"java_session",issues=?output.errors,"Native remote equipment has unsupported resources");
                    }
                    scene.configure_artwork(output.artwork.clone());
                    self.installed = Some(output);
                }
                Err(e) => {
                    bevy::log::warn!(target:"java_session",error=%e,"Native remote equipment preparation failed");
                    self.error = Some(e);
                }
            }
        }
    }
    pub fn entry(
        &self,
        e: &world::java::entities::Entity,
        gear: &client_world::java::equipment::Equipment,
    ) -> Option<&Entry> {
        self.installed
            .as_ref()?
            .entries
            .get(&e.identity)
            .filter(|v| v.signature == signature(e, gear))
    }
    pub fn ground_entry(&self, id: Identity, revision: u64) -> Option<&Entry> {
        self.installed
            .as_ref()?
            .entries
            .get(&id)
            .filter(|v| v.ground && v.signature.revision == revision)
    }
    pub fn diagnostic(&self) -> serde_json::Value {
        let w = self.shared.0.lock().unwrap();
        serde_json::json!({"revision":w.desired,"pending":w.request.is_some(),"players":self.installed.as_ref().map_or(0,|o|o.entries.values().filter(|e|!e.ground).count()),"ground_items":self.installed.as_ref().map_or(0,|o|o.entries.values().filter(|e|e.ground).count()),"draws":self.installed.as_ref().map_or(0,|o|o.entries.values().map(|e|e.draws.len()).sum::<usize>()),"error":self.error,"resource_errors":self.installed.as_ref().map(|o|&o.errors)})
    }
}
