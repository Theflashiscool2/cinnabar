//! Change-driven equipment artwork. One replaceable worker; world/inventory remain authoritative.
use super::assets;
use ::assets::registry::java::items::{Catalog, OwnerObservations, Selection, Stack, View};
use client_world::java::equipment::model_using;
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
#[derive(Clone)]
pub(super) struct Held {
    owner: Option<OwnerObservations>,
    selected: Result<Selection, String>,
}
pub(super) struct Player {
    id: Identity,
    signature: Signature,
    slots: [inventory::java::Slot; 8],
    hands: [Option<Held>; 2],
}
pub(super) struct Ground {
    id: Identity,
    revision: u64,
    stack: inventory::java::Stack,
}
pub(super) struct SlotSource {
    pub slot: u8,
    pub item: Arc<str>,
    pub using_this_item: Option<bool>,
}
impl SlotSource {
    fn matches_use(
        &self,
        gear: &client_world::java::equipment::Equipment,
        visual: &world::java::entities::visual::Visual,
    ) -> bool {
        self.using_this_item
            .is_none_or(|expected| model_using(visual, gear, self.slot) == expected)
    }
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
#[derive(Clone, Copy)]
pub(super) enum Attachment {
    Hand(bool),
    Skull,
}
pub(super) struct Draw {
    pub source: Option<SlotSource>,
    pub model: Option<Arc<render_model::actor::java_geometry::Model>>,
    pub rig: render_model::EntityRigId,
    pub attachment: Option<Attachment>,
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
    heads: Arc<::assets::registry::java::items::HeadTextures>,
}
#[derive(Default)]
struct Shared {
    closed: bool,
    desired: Option<u64>,
    request: Option<Request>,
    result: Option<(u64, Result<Output, String>)>,
}
#[derive(Clone, PartialEq, Eq)]
struct UseInput {
    identity: Identity,
    revision: u64,
    remaining: Option<i32>,
    using: [bool; 2],
}
#[derive(Clone, PartialEq, Eq)]
struct Inputs {
    equipment: u64,
    models: Vec<(Identity, bool)>,
    heads: u64,
    uses: Vec<UseInput>,
}
#[derive(Clone, PartialEq)]
struct SelectedHand {
    selected: Result<Selection, String>,
    using_this_item: Option<bool>,
}
#[derive(PartialEq)]
struct Source {
    equipment: u64,
    models: Vec<(Identity, bool)>,
    heads: u64,
    selections: Vec<(Identity, [Option<SelectedHand>; 2])>,
}
pub(super) struct State {
    catalog: Arc<Catalog>,
    shared: Arc<(Mutex<Shared>, Condvar)>,
    installed: Option<Output>,
    error: Option<String>,
    observed_source: Option<Inputs>,
    desired_source: Option<Source>,
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
            observed_source: None,
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
        heads: Arc<::assets::registry::java::items::HeadTextures>,
    ) {
        if self.error.is_some() {
            return;
        }
        let mut work = self.shared.0.lock().unwrap();
        let inputs = Inputs {
            equipment: state.equipment.revision(),
            models: skins.model_key(),
            heads: heads.revision,
            uses: state
                .entities
                .iter()
                .filter(|entity| entity.visual.player.is_some())
                .filter_map(|entity| {
                    let gear = state.equipment.get(entity.identity)?;
                    Some(UseInput {
                        identity: entity.identity,
                        revision: entity.visual.use_revision,
                        remaining: entity
                            .visual
                            .remote_use
                            .as_ref()
                            .and_then(|using| using.remaining),
                        using: [0, 1].map(|slot| model_using(&entity.visual, gear, slot)),
                    })
                })
                .collect(),
        };
        if self.observed_source.as_ref() != Some(&inputs) {
            self.observed_source = Some(inputs.clone());
            let players: Vec<_> = state
                .entities
                .iter()
                .filter(|entity| entity.visual.player.is_some())
                .filter_map(|entity| {
                    let gear = state.equipment.get(entity.identity)?;
                    let signature = signature(entity, gear, skins);
                    let hands = [0, 1].map(|slot| {
                        gear.slots[slot].as_ref().map(|stack| {
                            let owner = client_world::java::equipment::model_owner(
                                &entity.visual,
                                gear,
                                slot as u8,
                                Arc::clone(&entity.kind),
                            );
                            let left = (slot == 0) == signature.left;
                            let selected = self
                                .catalog
                                .select_in(
                                    &work::stack(stack),
                                    if left {
                                        View::ThirdPersonLeft
                                    } else {
                                        View::ThirdPersonRight
                                    },
                                    owner.as_ref(),
                                )
                                .map_err(|error| error.to_string());
                            Held { owner, selected }
                        })
                    });
                    Some(Player {
                        id: entity.identity,
                        signature,
                        slots: gear.slots.clone(),
                        hands,
                    })
                })
                .collect();
            let source = Source {
                equipment: inputs.equipment,
                models: inputs.models,
                heads: inputs.heads,
                selections: players
                    .iter()
                    .map(|player| {
                        (
                            player.id,
                            player.hands.each_ref().map(|hand| {
                                hand.as_ref().map(|hand| SelectedHand {
                                    selected: hand.selected.clone(),
                                    using_this_item: hand
                                        .owner
                                        .as_ref()
                                        .map(|owner| owner.using_this_item),
                                })
                            }),
                        )
                    })
                    .collect(),
            };
            if self.desired_source.as_ref() != Some(&source) {
                let Some(revision) = self.sequence.checked_add(1) else {
                    self.error = Some("Native equipment publication exhausted".into());
                    return;
                };
                self.sequence = revision;
                self.desired_source = Some(source);
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
                    .chain(state.equipment.pickups().filter_map(|pickup| {
                        Some(Ground {
                            id: pickup.identity,
                            revision: pickup.ground.revision,
                            stack: pickup.ground.stack.clone()?,
                        })
                    }))
                    .collect();
                work.desired = Some(revision);
                work.request = Some(Request {
                    revision,
                    players,
                    grounds,
                    heads,
                });
                work.result = None;
                self.shared.1.notify_one();
            }
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
    fn retained_charged_draw_is_hidden_immediately_when_native_use_stops() {
        let mut gear = client_world::java::equipment::Equipment::default();
        gear.slots[0] = Some(inventory::java::Stack {
            item: "minecraft:bow".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        });
        let mut visual = world::java::entities::visual::Visual::default();
        visual.observe_use(1).unwrap();
        visual.remote_use = Some(world::java::entities::visual::RemoteUse {
            item: "minecraft:bow".into(),
            item_token: (0, 0),
            observed_item: Some("minecraft:bow".into()),
            observed_token: Some((0, 0)),
            remaining: Some(71940),
        });
        let charged = SlotSource {
            slot: 0,
            item: "minecraft:bow".into(),
            using_this_item: Some(true),
        };
        assert!(charged.matches(&gear) && charged.matches_use(&gear, &visual));
        visual.observe_use(0).unwrap();
        assert!(
            charged.matches(&gear),
            "release retains the same authoritative item"
        );
        assert!(
            !charged.matches_use(&gear, &visual),
            "pending replacement artwork cannot retain a charged draw"
        );
        let idle = SlotSource {
            using_this_item: Some(false),
            ..charged
        };
        assert!(idle.matches_use(&gear, &visual));
    }
    #[test]
    fn retained_draws_retire_removed_or_replaced_authoritative_slots_immediately() {
        let source = SlotSource {
            slot: 0,
            item: "minecraft:diamond_sword".into(),
            using_this_item: None,
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
