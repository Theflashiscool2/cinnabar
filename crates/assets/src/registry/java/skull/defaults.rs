//! Default head textures from the same generated native facts used by player admission.
use serde::Deserialize;
use std::sync::OnceLock;
#[derive(Deserialize)]
struct Skin {
    texture: String,
}
#[derive(Deserialize)]
struct Facts {
    default_player_skins: Vec<Skin>,
    default_player_skin_index: usize,
}
struct Defaults {
    textures: Vec<String>,
    unprofiled: usize,
}
fn defaults() -> &'static Defaults {
    static DEFAULTS: OnceLock<Defaults> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        let facts: Facts = serde_json::from_str(include_str!(
            "../../../../../java-protocol/data/entity-facts.json"
        ))
        .expect("generated native player skin facts");
        assert!(facts.default_player_skin_index < facts.default_player_skins.len());
        let textures = facts
            .default_player_skins
            .into_iter()
            .map(|skin| {
                skin.texture
                    .strip_prefix("minecraft:")
                    .and_then(|p| p.strip_suffix(".png"))
                    .expect("native default player texture path")
                    .to_owned()
            })
            .collect();
        Defaults {
            textures,
            unprofiled: facts.default_player_skin_index,
        }
    })
}
pub fn all() -> &'static [String] {
    &defaults().textures
}
pub fn texture(index: usize) -> Option<&'static str> {
    all().get(index).map(String::as_str)
}
pub fn unprofiled() -> &'static str {
    texture(defaults().unprofiled).expect("native default player skin index")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_factory_default_is_not_the_nil_uuid_default() {
        // Independently probed DefaultPlayerSkin.getDefaultSkin and the native ordered table.
        assert_eq!(unprofiled(), "textures/entity/player/slim/steve");
        assert_eq!(texture(0), Some("textures/entity/player/slim/alex"));
        assert_ne!(unprofiled(), texture(0).unwrap());
        assert!(texture(usize::MAX).is_none());
    }
}
