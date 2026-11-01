//! Public Java skin-property resolution. No account credentials are sent to texture hosts.
use assets::registry::java::{skull::skin, textures::Image};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Operational budgets return explicit errors; they do not establish complete resource parity.
const PROPERTY_BUDGET: usize = 64 * 1024;
const PNG_BUDGET: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Texture(String);
impl Texture {
    /// Extracts the public SKIN resource. Unknown JSON fields are not interpreted as actions.
    pub fn from_property(encoded: &str) -> Result<Option<Self>, String> {
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
        Self::from_url(url).map(Some)
    }
    fn from_url(url: &str) -> Result<Self, String> {
        let url = reqwest::Url::parse(url).map_err(|_| "invalid Java skin URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str() != Some("textures.minecraft.net")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("unsupported Java skin URL; official texture host required".into());
        }
        let hash = url
            .path()
            .strip_prefix("/texture/")
            .ok_or("invalid Java skin texture path")?;
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("unsupported Java skin texture identity".into());
        }
        Ok(Self(hash.to_ascii_lowercase()))
    }
    /// Content-service identity, safe as a cache key; this is not a claimed verified file digest.
    pub fn key(&self) -> &str {
        &self.0
    }
    /// Official legacy HTTP references are upgraded to HTTPS before requesting bytes.
    pub fn url(&self) -> String {
        format!("https://textures.minecraft.net/texture/{}", self.0)
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
        skin::decode(&bytes).map_err(|_| "invalid Java head skin image".into())
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
        assert_eq!(t.key(), hash);
        assert_eq!(t.url(), url.replacen("http:", "https:", 1));
        for url in [
            "file:///tmp/skin.png".to_owned(),
            format!("https://textures.minecraft.net.attacker.invalid/texture/{hash}"),
            format!("https://user@textures.minecraft.net/texture/{hash}"),
            format!("https://textures.minecraft.net/texture/{hash}?next=http://localhost"),
            format!("https://textures.minecraft.net/texture/{hash}/extra"),
        ] {
            assert!(Texture::from_url(&url).is_err());
        }
    }
    #[test]
    fn texture_property_decodes_without_treating_text_as_paths() {
        let hash = "0123456789abcdef".repeat(4);
        let property = serde_json::json!({"textures":{"SKIN":{"url":format!("https://textures.minecraft.net/texture/{hash}")}}});
        let encoded = STANDARD.encode(serde_json::to_vec(&property).unwrap());
        assert_eq!(
            Texture::from_property(&encoded).unwrap().unwrap().key(),
            hash
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
