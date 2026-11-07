//! Public textures use the bounded downloader and existing GPU skin residency.
use client_shell::player_skin::java::{Texture, cache::Cache};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use world::java::{
    entities::{Entities, Identity},
    roster::Profile,
};
#[derive(Default)]
pub(super) struct Players {
    cache: Mutex<Cache>,
    profiles: BTreeMap<Identity, (Arc<Profile>, Option<(Texture, bool)>)>,
    ready: BTreeMap<Texture, render_api::SkinRgba8>,
    failures: Vec<String>,
}
impl Players {
    pub fn update(&mut self, scope: (u64, u64), now: Duration, entities: &Entities) {
        let live: BTreeSet<_> = entities.iter().map(|e| e.identity).collect();
        self.profiles.retain(|id, _| live.contains(id));
        let mut desired = BTreeSet::new();
        for e in entities.iter() {
            let Some(profile) = &e.visual.player else {
                continue;
            };
            let resolve = || {
                (
                    Arc::clone(profile),
                    profile
                        .properties
                        .iter()
                        .find(|p| p.name.as_ref() == "textures")
                        .and_then(|p| Texture::appearance(&p.value).ok().flatten()),
                )
            };
            let entry = self.profiles.entry(e.identity).or_insert_with(resolve);
            if !Arc::ptr_eq(&entry.0, profile) {
                *entry = resolve();
            }
            if let Some((texture, _)) = &entry.1 {
                desired.insert(texture.clone());
            }
        }
        let cache = self.cache.get_mut().unwrap();
        cache.update(scope, now, &desired);
        self.ready.retain(|key, _| desired.contains(key));
        let previous_failures = std::mem::take(&mut self.failures);
        for texture in desired {
            if let Some(error) = cache.failure(&texture)
                && self.failures.len() < 8
                && !self.failures.iter().any(|e| e == error)
            {
                self.failures.push(error.to_owned());
            }
            if let Some(image) = cache.image(&texture) {
                self.ready
                    .entry(texture)
                    .or_insert_with(|| render_api::SkinRgba8::new(image.rgba.clone().into()));
            }
        }
        if self.failures != previous_failures && !self.failures.is_empty() {
            bevy::log::warn!(target:"java_session",issues=?self.failures,"Native Java player skins unavailable; default skins remain visible");
        }
    }
    pub fn retire(&mut self) {
        self.cache.get_mut().unwrap().scope(None);
        self.profiles.clear();
        self.ready.clear();
        self.failures.clear();
    }
    #[cfg(feature = "developer-control")]
    pub fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({"ready_textures":self.ready.len(),"profiles":self.profiles.len(),"failures":self.failures})
    }
    pub fn model_key(&self) -> Vec<(Identity, bool)> {
        self.profiles
            .keys()
            .filter_map(|id| self.get(*id).map(|(_, slim)| (*id, slim)))
            .collect()
    }
    pub fn get(&self, identity: Identity) -> Option<(render_api::SkinRgba8, bool)> {
        let (texture, slim) = self.profiles.get(&identity)?.1.as_ref()?;
        Some((self.ready.get(texture)?.clone(), *slim))
    }
}

/// Frame-local indices must fit the shared renderer even when many actors share pixels.
pub(super) fn frame_layer(
    layers: &mut Vec<render_api::SkinRgba8>,
    skin: render_api::SkinRgba8,
) -> Option<u32> {
    if let Some(index) = layers.iter().position(|old| old == &skin) {
        return Some(index as u32);
    }
    if layers.len() >= render_model::MAX_RENDERED_PLAYERS {
        return None;
    }
    let index = layers.len() as u32;
    layers.push(skin);
    Some(index)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_skins_reuse_a_layer_and_overflow_cannot_invalidate_the_frame() {
        let skin = |id: u32| {
            let mut pixels = vec![255; 64 * 64 * 4];
            pixels[..4].copy_from_slice(&id.to_le_bytes());
            render_api::SkinRgba8::from(pixels)
        };
        let mut layers = Vec::new();
        assert_eq!(frame_layer(&mut layers, skin(0)), Some(0));
        assert_eq!(frame_layer(&mut layers, skin(0)), Some(0));
        for i in 1..render_model::MAX_RENDERED_PLAYERS {
            assert_eq!(frame_layer(&mut layers, skin(i as u32)), Some(i as u32));
        }
        assert_eq!(frame_layer(&mut layers, skin(u32::MAX)), None);
        assert_eq!(layers.len(), render_model::MAX_RENDERED_PLAYERS);
        assert_eq!(frame_layer(&mut layers, skin(0)), Some(0));
    }
}
