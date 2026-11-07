//! Public Java skin-property resolution. No account credentials are sent to texture hosts.
use assets::registry::java::{skull::skin, textures::Image};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
#[cfg(feature = "developer-control")]
mod fixtures;

/// Operational budgets return explicit errors; they do not establish complete resource parity.
pub(crate) const TEXTURE_HOST: &str = "textures.minecraft.net";
const PROPERTY_BUDGET: usize = 64 * 1024;
const PNG_BUDGET: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Texture(String);
impl Texture {
    /// Extracts the public SKIN resource. Unknown JSON fields are not interpreted as actions.
    pub fn from_property(encoded: &str) -> Result<Option<Self>, String> {
        Self::appearance(encoded).map(|skin| skin.map(|(texture, _)| texture))
    }
    /// Public texture and native arm model; absent/unknown models select wide arms.
    pub fn appearance(encoded: &str) -> Result<Option<(Self, bool)>, String> {
        if encoded.len() > PROPERTY_BUDGET {
            return Err("Java skin property exceeds processing budget".into());
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| "invalid Java skin property base64")?;
        let data: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "invalid Java skin property JSON")?;
        if !data.is_object() {
            return Err("Java skin property object expected".into());
        }
        let Some(textures) = data.get("textures") else {
            return Ok(None);
        };
        if !textures.is_object() {
            return Err("Java skin textures object expected".into());
        }
        let Some(skin) = textures.get("SKIN") else {
            return Ok(None);
        };
        let url = skin
            .get("url")
            .and_then(serde_json::Value::as_str)
            .ok_or("Java skin URL missing")?;
        let slim = skin
            .get("metadata")
            .and_then(|m| m.get("model"))
            .and_then(serde_json::Value::as_str)
            == Some("slim");
        Self::from_url(url).map(|texture| Some((texture, slim)))
    }
    fn from_url(url: &str) -> Result<Self, String> {
        let mut url = reqwest::Url::parse(url).map_err(|_| "invalid Java skin URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str() != Some(TEXTURE_HOST)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            return Err("unsupported Java skin URL; official texture host required".into());
        }
        if !url.path().starts_with("/texture/") {
            return Err("invalid Java skin texture path".into());
        }
        // The pinned authlib accepts texture URIs by the discovered service prefix;
        // MinecraftProfileTexture.getHash does not require a 64-character digest.
        url.set_scheme("https")
            .map_err(|_| "invalid Java skin scheme")?;
        url.set_fragment(None);
        Ok(Self(url.to_string()))
    }

    /// Canonical public URI cache identity, never a filesystem path or verified file digest.
    pub fn key(&self) -> &str {
        &self.0
    }
    /// Private render-resource key; server names never become paths or override pack textures.
    pub fn head_atlas_name(&self) -> String {
        use sha2::{Digest, Sha256};
        format!("java_head_skin/{:x}", Sha256::digest(self.0.as_bytes()))
    }
    /// Official legacy HTTP references are upgraded to HTTPS before requesting bytes.
    pub fn url(&self) -> String {
        self.0.clone()
    }
}

/// Reusable HTTP connection pool for a host-owned, bounded skin worker queue.
pub struct Downloader(reqwest::Client);
impl Downloader {
    pub fn new() -> Result<Self, String> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .timeout(Duration::from_secs(15))
            .build()
            .map(Self)
            .map_err(|_| "could not initialize Java skin transport".into())
    }
    /// Callers compare world/resource generations before publishing results. Cancellation is
    /// checked before requesting, during bounded reads and before allocating normalized pixels.
    pub async fn head(&self, texture: &Texture, cancelled: &AtomicBool) -> Result<Image, String> {
        let check = || {
            if cancelled.load(Ordering::Relaxed) {
                Err("Java skin request cancelled".to_owned())
            } else {
                Ok(())
            }
        };
        check()?;
        #[cfg(feature = "developer-control")]
        if let Some(image) = fixtures::head(texture, cancelled)? {
            return Ok(image);
        }
        let mut response = self
            .0
            .get(texture.url())
            .send()
            .await
            .map_err(|_| "Java skin network request failed")?;
        if !response.status().is_success() {
            return Err(format!("Java skin HTTP {}", response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|n| n > PNG_BUDGET as u64)
        {
            return Err("Java skin exceeds download budget".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Java skin response failed")?
        {
            check()?;
            if bytes.len().saturating_add(chunk.len()) > PNG_BUDGET {
                return Err("Java skin exceeds download budget".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        check()?;
        player_image(&bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_legacy_url_upgrades_and_untrusted_routes_are_rejected() {
        let hash = "0123456789abcdef".repeat(4);
        let url = format!("http://textures.minecraft.net/texture/{hash}");
        let t = Texture::from_url(&url).unwrap();
        assert_eq!(t.key(), url.replacen("http:", "https:", 1));
        assert_eq!(t.url(), url.replacen("http:", "https:", 1));
        for url in [
            "file:///tmp/skin.png".to_owned(),
            format!("https://textures.minecraft.net.attacker.invalid/texture/{hash}"),
            format!("https://user@textures.minecraft.net/texture/{hash}"),
            "https://textures.minecraft.net/not-a-texture/abc".into(),
        ] {
            assert!(Texture::from_url(&url).is_err());
        }
    }
    #[test]
    fn native_uri_admission_has_no_digest_length_assumption_and_private_keys_are_hashed() {
        let t =
            Texture::from_url("http://textures.minecraft.net/texture/legacy.png?variant=1#ignored")
                .unwrap();
        assert_eq!(
            t.url(),
            "https://textures.minecraft.net/texture/legacy.png?variant=1"
        );
        let name = t.head_atlas_name();
        assert!(name.starts_with("java_head_skin/"));
        assert_eq!(name.len(), "java_head_skin/".len() + 64);
        assert!(
            name.trim_start_matches("java_head_skin/")
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
        );
        let other =
            Texture::from_url("https://textures.minecraft.net/texture/legacy.png?variant=2")
                .unwrap();
        assert_ne!(name, other.head_atlas_name());
    }
    #[test]
    fn texture_property_decodes_without_treating_text_as_paths() {
        let hash = "0123456789abcdef".repeat(4);
        let property = serde_json::json!({"textures":{"SKIN":{"url":format!("https://textures.minecraft.net/texture/{hash}")}}});
        let encoded = STANDARD.encode(serde_json::to_vec(&property).unwrap());
        assert_eq!(
            Texture::from_property(&encoded).unwrap().unwrap().key(),
            format!("https://textures.minecraft.net/texture/{hash}")
        );
        assert_eq!(
            Texture::from_property(&STANDARD.encode(b"{\"textures\":{}}")).unwrap(),
            None
        );
        assert!(Texture::from_property("not base64!").is_err());
        assert!(Texture::from_property(&STANDARD.encode(b"[]")).is_err());
        assert!(Texture::from_property(&STANDARD.encode(b"{\"textures\":1}")).is_err());
        assert!(Texture::from_property(&"x".repeat(PROPERTY_BUDGET + 1)).is_err());
    }
}

pub mod pool;

pub mod cache;

pub mod profiles;

/// Reuses the guarded head decoder and expands native whole-player body regions.
fn player_image(bytes: &[u8]) -> Result<Image, String> {
    let mut image = skin::decode(bytes).map_err(|_| "invalid Java skin image")?;
    if u32::from_be_bytes(bytes[20..24].try_into().unwrap()) == 32 {
        image.rgba = render_api::expand_legacy_skin_rgba8(&image.rgba[..64 * 32 * 4], 64);
    }
    for (x0, y0, x1, y1) in [(0, 16, 64, 32), (16, 48, 48, 64)] {
        for y in y0..y1 {
            for x in x0..x1 {
                image.rgba[(y * 64 + x) * 4 + 3] = 255;
            }
        }
    }
    Ok(image)
}

#[cfg(test)]
mod player_tests {
    use super::*;
    fn png(height: u32) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            64,
            height,
            image::Rgba([57, 99, 133, 12]),
        ))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
        out.into_inner()
    }
    #[test]
    fn whole_body_alpha_and_legacy_limbs_are_prepared_without_losing_hat_alpha() {
        for height in [32, 64] {
            let image = player_image(&png(height)).unwrap();
            assert_eq!(
                (image.width, image.height, image.rgba.len()),
                (64, 64, 64 * 64 * 4)
            );
            for (x, y) in [(8, 8), (8, 24), (24, 56), (40, 56)] {
                assert_eq!(image.rgba[(y * 64 + x) * 4 + 3], 255);
            }
            assert_eq!(image.rgba[(8 * 64 + 40) * 4 + 3], 12);
            assert_eq!(&image.rgba[(56 * 64 + 24) * 4..][..3], &[57, 99, 133]);
        }
    }
    #[test]
    fn property_selects_slim_without_changing_public_texture_identity() {
        let encoded = |model| {
            STANDARD.encode(serde_json::json!({"textures":{"SKIN":{"url":"https://textures.minecraft.net/texture/fixture","metadata":{"model":model}}}}).to_string())
        };
        let (a, slim) = Texture::appearance(&encoded("slim")).unwrap().unwrap();
        let (b, wide) = Texture::appearance(&encoded("default")).unwrap().unwrap();
        assert!(slim);
        assert!(!wide);
        assert_eq!(a, b);
    }
}
