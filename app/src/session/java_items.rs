//! Inventory head skins reuse the bounded public-profile/skin services and native defaults.
use assets::registry::java::items::{HeadTexture, HeadTextures, profile_key};
use client_shell::player_skin::java::{cache, profiles};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
#[derive(Default)]
pub(crate) struct Heads {
    scope: Option<(u64, u64)>,
    receipt: Option<u64>,
    descriptors: BTreeMap<String, super::java_heads::Head>,
    profiles: profiles::Cache,
    skins: cache::Cache,
    identity: Vec<(String, String)>,
    failures: BTreeSet<(String, String)>,
    output: Arc<HeadTextures>,
}
impl Heads {
    pub fn clear(&mut self) {
        if self.scope.take().is_none() {
            return;
        }
        self.receipt = None;
        self.descriptors.clear();
        self.identity.clear();
        self.failures.clear();
        self.profiles.scope(None);
        self.skins.scope(None);
        self.output = Arc::new(HeadTextures::default());
    }
    pub fn update(
        &mut self,
        scope: (u64, u64),
        world: &client_world::java::WorldState,
        now: Duration,
    ) -> Arc<HeadTextures> {
        if self.scope != Some(scope) {
            self.clear();
            self.scope = Some(scope);
        }
        if self.receipt != Some(world.inventory.revision) {
            self.descriptors.clear();
            self.failures.clear();
            for menu in world
                .inventory
                .player
                .iter()
                .chain(world.inventory.opened.iter())
            {
                let stacks = (0..menu.len())
                    .filter_map(|i| world.inventory.slot(menu.container, i))
                    .chain(menu.carried.iter());
                for stack in stacks.flatten() {
                    if stack.removed.contains("minecraft:profile") {
                        continue;
                    }
                    if let Some(value) = stack
                        .added
                        .get("minecraft:profile")
                        .filter(|v| !v.is_null())
                    {
                        match super::java_heads::Head::item(value) {
                            Ok(head) => {
                                self.descriptors.insert(profile_key(value), head);
                            }
                            Err(error) => {
                                bevy::log::warn!(target:"java_session",%error,"Invalid native item head profile")
                            }
                        }
                    }
                }
            }
            self.receipt = Some(world.inventory.revision);
        }
        let queries = self
            .descriptors
            .values()
            .filter_map(|h| h.query.clone())
            .collect();
        self.profiles.update(scope, now, &queries);
        for head in self.descriptors.values_mut() {
            head.resolve(&self.profiles);
        }
        let desired = self
            .descriptors
            .values()
            .filter_map(|h| h.texture.clone())
            .collect::<BTreeSet<_>>();
        self.skins.update(scope, now, &desired);
        let images = self.skins.images(&desired);
        let mut identity = Vec::new();
        let mut textures = BTreeMap::new();
        for (key, head) in &self.descriptors {
            let failure = head
                .query
                .as_ref()
                .and_then(|q| self.profiles.failure(q))
                .or_else(|| head.texture.as_ref().and_then(|t| self.skins.failure(t)));
            if let Some(error) = failure
                && self.failures.insert((key.clone(), error.into()))
            {
                bevy::log::warn!(target:"java_session",%error,"Native item head uses UUID default while public resource is unavailable");
            }
            if head.patch.is_none()
                && let Some((name, image)) = head
                    .texture
                    .as_ref()
                    .and_then(|t| images.get_key_value(&t.head_atlas_name()))
            {
                identity.push((key.clone(), name.clone()));
                textures.insert(key.clone(), HeadTexture::Image(image.clone()));
            } else {
                let path = head
                    .patch
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| head.default_texture().into());
                identity.push((key.clone(), path.clone()));
                textures.insert(key.clone(), HeadTexture::Pack(path));
            }
        }
        if self.identity != identity {
            self.identity = identity;
            self.output = Arc::new(HeadTextures {
                revision: self.output.revision.wrapping_add(1),
                textures,
            });
        }
        self.output.clone()
    }
}
