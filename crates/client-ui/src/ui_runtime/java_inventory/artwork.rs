//! Change-driven native item artwork on one bounded worker; authoritative inventory stays in its owner.
use super::{Projection, UiRuntime};
use crate::ui_runtime::presentation::{SessionIcon, SessionIcons};
use assets::registry::java::items::{Catalog, HeadTextures, Stack};
use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex},
};
mod work;
use work::{Request, Shared, worker};
pub(in crate::ui_runtime) struct State {
    catalog: Arc<Catalog>,
    shared: Arc<(Mutex<Shared>, Condvar)>,
}
impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeJavaItemArtwork")
    }
}
impl Drop for State {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap().closed = true;
        wake.notify_one();
    }
}
impl State {
    fn new(catalog: Arc<Catalog>) -> Self {
        let shared = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let work = shared.clone();
        let source = catalog.clone();
        let spawned = std::thread::Builder::new()
            .name("java-item-artwork".into())
            .spawn(move || worker(source, work));
        if let Err(error) = spawned {
            tracing::warn!(target:"java_session",%error,"Native item artwork worker unavailable");
            shared.0.lock().unwrap().closed = true;
        }
        Self { catalog, shared }
    }
    fn refresh(
        &self,
        projection: &Projection,
        world: &client_world::java::WorldState,
        heads: Arc<HeadTextures>,
    ) -> Option<Arc<SessionIcons>> {
        let (session, epoch, revision) = projection.key?;
        let ticket = (session, epoch, revision, heads.revision);
        let mut work = self.shared.0.lock().unwrap();
        if work.closed {
            return None;
        }
        if work.desired != Some(ticket) {
            let mut stacks = BTreeMap::new();
            let all = projection
                .player
                .iter()
                .chain(projection.hotbar.iter())
                .chain(std::iter::once(&projection.offhand))
                .chain(std::iter::once(&projection.player_carried))
                .chain(
                    projection
                        .opened
                        .iter()
                        .flat_map(|m| m.slots.iter().chain(std::iter::once(&m.carried))),
                );
            for stack in all.flatten().flatten() {
                let mut value = Stack {
                    item: stack.item.clone(),
                    count: stack.count,
                    added: stack.added.clone(),
                    removed: stack.removed.clone(),
                };
                let key = value.key();
                normalize_registry_components(&mut value, world);
                stacks.entry(key).or_insert(value);
            }
            work.desired = Some(ticket);
            work.request = Some(Request {
                ticket,
                stacks,
                heads,
            });
            work.result = None;
            self.shared.1.notify_one();
        }
        work.result.take().and_then(|(key, icons)| {
            if key == ticket {
                work.installed = Some(icons.clone());
                Some(icons)
            } else {
                None
            }
        })
    }
}
fn normalize_registry_components(stack: &mut Stack, world: &client_world::java::WorldState) {
    if let Some(layers) = stack
        .added
        .get_mut("minecraft:banner_patterns")
        .and_then(serde_json::Value::as_array_mut)
    {
        for layer in layers {
            let pattern = &mut layer["pattern"];
            let asset = pattern
                .get("id")
                .and_then(serde_json::Value::as_u64)
                .and_then(|id| world.registry_asset("minecraft:banner_pattern", id))
                .map(str::to_owned)
                .or_else(|| {
                    pattern
                        .get("inline")
                        .and_then(|v| v.get("asset"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                });
            if let Some(asset) = asset {
                *pattern = serde_json::json!({"asset_id":asset});
            }
        }
    }
    if let Some(sides) = stack
        .added
        .get_mut("minecraft:pot_decorations")
        .and_then(|v| v.get_mut("sides"))
        .and_then(serde_json::Value::as_array_mut)
    {
        for template in sides {
            if let Some(value) = template
                .get_mut("added")
                .and_then(|v| v.get_mut("minecraft:provides_pottery_pattern"))
                && let Some(asset) = value
                    .as_u64()
                    .and_then(|id| world.registry_asset("minecraft:decorated_pot_pattern", id))
            {
                *value = serde_json::json!({"asset_id":asset});
            }
        }
    }
}
impl UiRuntime {
    /// Bounded operational state; no component payloads or authentication data are retained.
    pub fn java_item_artwork_diagnostics(&self) -> serde_json::Value {
        let Some(state) = self.session.java_item_artwork.as_ref() else {
            return serde_json::Value::Null;
        };
        let work = state.shared.0.lock().unwrap();
        let icons = work.installed.as_ref();
        let misses = icons
            .map(|i| i.misses.iter().take(16).collect::<BTreeMap<_, _>>())
            .unwrap_or_default();
        serde_json::json!({"desired":work.desired,"queued":work.request.is_some(),"closed":work.closed,"icons":icons.map_or(0,|i|i.icons.len()),"missing":icons.map_or(0,|i|i.misses.len()),"examples":misses})
    }
    pub(in crate::ui_runtime) fn clear_java_item_artwork(&mut self) {
        if let Some(old) = self.session.java_item_artwork.take() {
            let clear = {
                let mut work = old.shared.0.lock().unwrap();
                work.closed = true;
                work.installed
                    .as_ref()
                    .zip(self.session.session_icons.as_ref())
                    .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
            };
            old.shared.1.notify_one();
            if clear {
                self.set_session_icons(None);
            }
        }
    }
    /// Publishes only the matching resource/world generation. Revisions replace queued work, never transactions.
    pub fn publish_java_item_catalog(
        &mut self,
        session: u64,
        epoch: u64,
        catalog: Arc<Catalog>,
        world: &client_world::java::WorldState,
        heads: Arc<HeadTextures>,
    ) -> bool {
        if !self
            .java_hud()
            .is_some_and(|h| (h.session, h.epoch) == (session, epoch))
        {
            return false;
        }
        let changed = self
            .session
            .java_item_artwork
            .as_ref()
            .is_none_or(|s| !Arc::ptr_eq(&s.catalog, &catalog));
        if changed {
            self.clear_java_item_artwork();
            self.session.java_item_artwork = Some(Arc::new(State::new(catalog)));
        }
        if let Some(icons) = self
            .session
            .java_item_artwork
            .as_ref()
            .and_then(|s| s.refresh(&self.session.java_inventory, world, heads))
        {
            self.set_session_icons(Some(icons));
        }
        true
    }
}
