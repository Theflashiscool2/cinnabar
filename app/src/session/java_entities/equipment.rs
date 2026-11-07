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
pub(super) struct SlotSource {
    pub slot: u8,
    pub item: Arc<str>,
}
impl SlotSource {
    fn matches(&self, gear: &client_world::java::equipment::Equipment) -> bool {
        gear.slots
            .get(usize::from(self.slot))
            .and_then(Option::as_ref)
            .is_some_and(|stack| stack.item.as_str() == self.item.as_ref())
    }
}
impl Signature {
    /// Keep the last committed graphics while an ordinary same-body update is prepared.
    fn usable(self, next: Self, failed: bool) -> bool {
        self == next
            || (!failed
                && self.revision < next.revision
                && self.slim == next.slim
                && self.left == next.left)
    }
}
pub(super) struct Draw {
    pub source: Option<SlotSource>,
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
    desired_source: Option<(u64, Vec<(Identity, bool)>)>,
    sequence: u64,
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
    skins: &super::skins::Players,
) -> Signature {
    Signature {
        revision: gear.revision,
        slim: skins.get(e.identity).map_or_else(
            || {
                e.visual
                    .player
                    .as_ref()
                    .is_some_and(|p| p.default_skin.slim)
            },
            |(_, slim)| slim,
        ),
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
            desired_source: None,
            sequence: 0,
        }
    }
    pub fn matches(&self, catalog: &Arc<Catalog>) -> bool {
        Arc::ptr_eq(&self.catalog, catalog)
    }
    pub fn update(
        &mut self,
        state: &client_world::java::WorldState,
        scene: &mut ActorRenderScene,
        skins: &super::skins::Players,
    ) {
        if self.error.is_some() {
            return;
        }
        let mut work = self.shared.0.lock().unwrap();
        let source = (state.equipment.revision(), skins.model_key());
        if self.desired_source.as_ref() != Some(&source) {
            let Some(revision) = self.sequence.checked_add(1) else {
                self.error = Some("Native equipment publication exhausted".into());
                return;
            };
            self.sequence = revision;
            self.desired_source = Some(source);
            let players = state
                .entities
                .iter()
                .filter(|e| e.visual.player.is_some())
                .filter_map(|e| {
                    let gear = state.equipment.get(e.identity)?;
                    Some(Player {
                        id: e.identity,
                        signature: signature(e, gear, skins),
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
        skins: &super::skins::Players,
    ) -> Option<&Entry> {
        self.installed
            .as_ref()?
            .entries
            .get(&e.identity)
            .filter(|v| {
                !v.ground
                    && v.signature
                        .usable(signature(e, gear, skins), self.error.is_some())
            })
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

#[cfg(test)]
mod continuity_tests {
    use super::*;
    #[test]
    fn committed_same_body_resources_survive_a_pending_revision_without_crossing_fences() {
        let old = Signature {
            revision: 1,
            slim: false,
            left: false,
        };
        let next = Signature { revision: 2, ..old };
        assert!(old.usable(next, false));
        assert!(!old.usable(next, true));
        assert!(!next.usable(old, false));
        assert!(!old.usable(Signature { slim: true, ..next }, false));
        assert!(!old.usable(Signature { left: true, ..next }, false));
        assert!(old.usable(old, true));
    }
    #[test]
    fn retained_draws_retire_removed_or_replaced_authoritative_slots_immediately() {
        let source = SlotSource {
            slot: 0,
            item: "minecraft:diamond_sword".into(),
        };
        let mut gear = client_world::java::equipment::Equipment::default();
        assert!(!source.matches(&gear));
        gear.slots[0] = Some(inventory::java::Stack {
            item: "minecraft:diamond_sword".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        });
        assert!(source.matches(&gear));
        gear.slots[0]
            .as_mut()
            .unwrap()
            .added
            .insert("minecraft:damage".into(), serde_json::json!(2));
        assert!(
            source.matches(&gear),
            "durability must not blank the last committed equipment"
        );
        gear.slots[0].as_mut().unwrap().item = "minecraft:shield".into();
        assert!(!source.matches(&gear));
        gear.slots[0] = None;
        assert!(!source.matches(&gear));
    }
}
