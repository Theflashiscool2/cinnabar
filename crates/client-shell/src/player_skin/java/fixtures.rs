//! Explicit developer-only local transport for graphical skin acceptance.
//! Profile parsing and official-host validation still run before this boundary.
use super::{Image, PNG_BUDGET, Texture, player_image};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    file: String,
    sha256: String,
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|_| "Java skin fixture file unavailable")?;
    if !file
        .metadata()
        .map_err(|_| "Java skin fixture metadata unavailable")?
        .is_file()
    {
        return Err("Java skin fixture requires a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Java skin fixture read failed")?;
    if bytes.len() > limit {
        return Err("Java skin fixture exceeds budget".into());
    }
    Ok(bytes)
}
fn load(
    manifest: &Path,
    texture: &Texture,
    cancelled: &AtomicBool,
) -> Result<Option<Image>, String> {
    let check = || {
        if cancelled.load(Ordering::Relaxed) {
            Err("Java skin fixture cancelled".to_owned())
        } else {
            Ok(())
        }
    };
    check()?;
    let entries: BTreeMap<String, Entry> =
        serde_json::from_slice(&read_bounded(manifest, 64 * 1024)?)
            .map_err(|_| "Invalid Java skin fixture manifest")?;
    let Some(entry) = entries.get(texture.key()) else {
        return Ok(None);
    };
    let file = Path::new(&entry.file);
    if !matches!(
        file.components().collect::<Vec<_>>().as_slice(),
        [Component::Normal(_)]
    ) {
        return Err("Java skin fixture requires one local filename".into());
    }
    let parent = manifest
        .parent()
        .ok_or("Java skin fixture directory unavailable")?
        .canonicalize()
        .map_err(|_| "Java skin fixture directory unavailable")?;
    let path = parent
        .join(file)
        .canonicalize()
        .map_err(|_| "Java skin fixture file unavailable")?;
    if path.parent() != Some(parent.as_path()) {
        return Err("Java skin fixture escaped its directory".into());
    }
    check()?;
    let bytes = read_bounded(&path, PNG_BUDGET)?;
    if format!("{:x}", Sha256::digest(&bytes)) != entry.sha256 {
        return Err("Java skin fixture checksum mismatch".into());
    }
    check()?;
    let image = player_image(&bytes)?;
    check()?;
    Ok(Some(image))
}
pub(super) fn head(texture: &Texture, cancelled: &AtomicBool) -> Result<Option<Image>, String> {
    if std::env::var_os("CINNABAR_JAVA_OFFLINE_FIXTURE").is_none() {
        return Ok(None);
    }
    let Some(path) = std::env::var_os("CINNABAR_JAVA_TEXTURE_FIXTURES") else {
        return Ok(None);
    };
    load(Path::new(&path), texture, cancelled)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_is_bounded_cancelled_and_cannot_escape_or_hide_bad_checksums() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("textures.json");
        let texture = Texture::from_url("https://textures.minecraft.net/texture/fixture").unwrap();
        let entry = |file: &str, hash: &str| {
            serde_json::json!({texture.key():{"file":file,"sha256":hash}}).to_string()
        };
        let cancelled = AtomicBool::new(false);
        std::fs::write(dir.path().join("skin.png"), b"not a PNG").unwrap();
        std::fs::write(&manifest, entry("../skin.png", "invalid")).unwrap();
        assert!(
            load(&manifest, &texture, &cancelled)
                .unwrap_err()
                .contains("one local filename")
        );
        std::fs::write(&manifest, entry("skin.png", "invalid")).unwrap();
        assert!(
            load(&manifest, &texture, &cancelled)
                .unwrap_err()
                .contains("checksum")
        );
        cancelled.store(true, Ordering::Relaxed);
        assert!(
            load(&manifest, &texture, &cancelled)
                .unwrap_err()
                .contains("cancelled")
        );
        cancelled.store(false, Ordering::Relaxed);
        std::fs::write(&manifest, vec![b' '; 64 * 1024 + 1]).unwrap();
        assert!(
            load(&manifest, &texture, &cancelled)
                .unwrap_err()
                .contains("budget")
        );
    }
}
