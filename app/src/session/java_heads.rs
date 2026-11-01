//! Change-driven placed player-head descriptors; public image work belongs to the shell.
use assets::registry::java::skull::defaults;
use client_shell::player_skin::java::Texture;
use render_model::block_entity::java_profile::Profile;
use std::{collections::BTreeSet, sync::Arc};
use world::java::Terrain;

pub(super) struct Head {
    reported_failure: bool,
    generation: Option<u64>,
    receipt: Option<u64>,
    default: Arc<str>,
    pub texture: Option<Texture>,
    pub query: Option<client_shell::player_skin::java::profiles::Query>,
    pub patch: Option<assets::registry::java::source::Identifier>,
}
impl Head {
    pub fn new(
        terrain: &Terrain,
        position: [i32; 3],
        data: Option<&world::NbtValue>,
    ) -> Result<Self, String> {
        let profile = Profile::from_block_entity(data)?;
        let default = if profile.present {
            let index = world::java::roster::default_skin_index(
                profile.id.unwrap_or([0; 16]),
                defaults::all().len(),
            )
            .ok_or("Java default head texture index invalid")?;
            defaults::texture(index).ok_or("Java default head texture missing")?
        } else {
            defaults::unprofiled()
        };
        let texture = profile
            .textures
            .as_deref()
            .map(Texture::from_property)
            .transpose()?
            .flatten();
        let query = if !profile.has_properties && (profile.name.is_some() != profile.id.is_some()) {
            use client_shell::player_skin::java::profiles::Query;
            match profile.id {
                Some(id) => Some(Query::Id(id)),
                None => profile.name.map(|name| Query::Name(name.into())),
            }
        } else {
            None
        };
        Ok(Self {
            reported_failure: false,
            generation: terrain.block_generation(position),
            receipt: terrain.block_entity_revision(position),
            default: default.into(),
            texture,
            query,
            patch: profile
                .texture_patch
                .as_deref()
                .map(assets::registry::java::source::Identifier::parse)
                .transpose()
                .map_err(|e| e.to_string())?,
        })
    }
    pub fn resolve(&mut self, profiles: &client_shell::player_skin::java::profiles::Cache) {
        if let Some(resolved) = self.query.as_ref().and_then(|q| profiles.get(q)) {
            self.texture = resolved.texture.clone();
            if let Some(texture) =
                world::java::roster::default_skin_index(resolved.id, defaults::all().len())
                    .and_then(defaults::texture)
            {
                self.default = texture.into();
            }
        }
    }
    pub fn report_failure(
        &mut self,
        position: [i32; 3],
        skins: &client_shell::player_skin::java::cache::Cache,
        profiles: &client_shell::player_skin::java::profiles::Cache,
    ) {
        if self.reported_failure {
            return;
        }
        if let Some(error) = self
            .texture
            .as_ref()
            .and_then(|t| skins.failure(t))
            .or_else(|| self.query.as_ref().and_then(|q| profiles.failure(q)))
        {
            bevy::prelude::warn!(?position,%error,"Java player head uses native default while public resource is unavailable");
            self.reported_failure = true;
        }
    }
    pub fn current(&self, terrain: &Terrain, position: [i32; 3]) -> bool {
        terrain.block_generation(position) == self.generation
            && self.generation.is_some()
            && terrain.block_entity_revision(position) == self.receipt
    }
    pub fn atlas_texture(&self, bound: &BTreeSet<String>) -> Arc<str> {
        if let Some(patch) = &self.patch {
            let key = patch_key(patch);
            if bound.contains(&key) {
                return key.into();
            }
        } else if let Some(texture) = &self.texture {
            let key = texture.head_atlas_name();
            if bound.contains(&key) {
                return key.into();
            }
        }
        Arc::clone(&self.default)
    }
}

pub(super) fn patch_key(id: &assets::registry::java::source::Identifier) -> String {
    format!("java_head_resource/{id}")
}
/// Reads through the same pinned archive boundary as Java terrain, entirely on the worker.
pub(super) fn prepare_patches(
    images: &mut std::collections::BTreeMap<String, Arc<assets::registry::java::textures::Image>>,
    patches: &[assets::registry::java::source::Identifier],
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let check = || {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            Err("Java artwork preparation cancelled".to_owned())
        } else {
            Ok(())
        }
    };
    check()?;
    if patches.is_empty() {
        return Ok(());
    }
    use assets::registry::java::source::{Jar, Source};
    use java_protocol::target;
    let root = launcher::install_layout::discover_java_assets().map_err(|e| e.to_string())?;
    let mut jar = Jar::open(
        &root.join("client.jar"),
        target::CLIENT_ASSETS_SHA256,
        target::CLIENT_ASSETS_BYTES,
    )
    .map_err(|e| e.to_string())?;
    for id in patches {
        check()?;
        let bytes = jar
            .read(&id.asset_path("textures", "png"))
            .map_err(|e| e.to_string())?;
        let image =
            assets::registry::java::skull::skin::decode(&bytes).map_err(|e| e.to_string())?;
        use assets::registry::java::skull::skin::SIDE;
        if image.width != SIDE as u32 || image.height != SIDE as u32 {
            return Err("Java local head patch must be 64x64".into());
        }
        check()?;
        images.insert(patch_key(id), Arc::new(image));
    }
    check()
}
