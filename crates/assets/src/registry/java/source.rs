//! Runtime-only Java resources. Reading ZIP entries never extracts server-controlled paths.
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier {
    namespace: String,
    path: String,
}
impl Identifier {
    pub fn parse(value: &str) -> Result<Self, Error> {
        let (namespace, path) = value.split_once(':').unwrap_or(("minecraft", value));
        let valid = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c);
        if namespace.is_empty()
            || !namespace.bytes().all(valid)
            || path.is_empty()
            || !path.bytes().all(|c| valid(c) || c == b'/')
            || path
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(Error::InvalidIdentifier(value.to_owned()));
        }
        Ok(Self {
            namespace: namespace.into(),
            path: path.into(),
        })
    }
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn asset_path(&self, folder: &str, extension: &str) -> String {
        format!(
            "assets/{}/{}/{}.{}",
            self.namespace, folder, self.path, extension
        )
    }
}
impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid Java resource identifier: {0}")]
    InvalidIdentifier(String),
    #[error("Java runtime assets do not match the pinned client archive")]
    Hash,
    #[error("Java resource I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("Java resource ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Java resource {0} exceeds the parsing budget")]
    Budget(String),
    #[error("Java texture PNG: {0}")]
    Png(#[from] png::DecodingError),
    #[error("Java graphics resource: {0}")]
    Graphics(#[from] crate::AssetError),
    #[error("Java resource JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported or invalid Java model: {0}")]
    Model(String),
    #[error("{0}")]
    ModelBatch(super::graphics::audit::Report),
}
/// Resource access runs on asset workers; callers never do file work on the render thread.
pub trait Source {
    fn cancelled(&self) -> bool {
        false
    }
    fn read(&mut self, path: &str) -> Result<Vec<u8>, Error>;
    /// Bounded resource keys, never extracted filesystem paths. Sources without enumeration
    /// fail explicitly when a renderer needs data-driven resource discovery.
    fn entries(&mut self, _prefix: &str) -> Result<Vec<String>, Error> {
        Err(Error::Model("Java resource enumeration unavailable".into()))
    }
    fn optional_json(&mut self, path: &str) -> Result<Option<serde_json::Value>, Error> {
        match self.read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(Error::Zip(zip::result::ZipError::FileNotFound)) => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn json(&mut self, path: &str) -> Result<serde_json::Value, Error> {
        Ok(serde_json::from_slice(&self.read(path)?)?)
    }
}
pub struct Jar {
    archive: zip::ZipArchive<File>,
}
impl Jar {
    /// The SHA and length come from the Java target manifest, not from the supplied archive.
    pub fn open(path: &Path, sha256: &str, expected_bytes: u64) -> Result<Self, Error> {
        let mut file = File::open(path).map_err(|error| {
            std::io::Error::new(error.kind(), format!("{}: {error}", path.display()))
        })?;
        if file.metadata()?.len() != expected_bytes {
            return Err(Error::Hash);
        }
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        if format!("{:x}", hash.finalize()) != sha256 {
            return Err(Error::Hash);
        }
        file.seek(SeekFrom::Start(0))?;
        Ok(Self {
            archive: zip::ZipArchive::new(file)?,
        })
    }
}
impl Source for Jar {
    fn entries(&mut self, prefix: &str) -> Result<Vec<String>, Error> {
        let entries: Vec<_> = self
            .archive
            .file_names()
            .filter(|p| p.starts_with(prefix) && !p.ends_with('/'))
            .take(4097)
            .map(str::to_owned)
            .collect();
        if entries.len() > 4096 {
            return Err(Error::Budget("Java resource enumeration".into()));
        }
        Ok(entries)
    }
    fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        // A provisional per-resource parser budget, not a pack compatibility claim.
        const MAX_RESOURCE: u64 = 16 * 1024 * 1024;
        let entry = self.archive.by_name(path)?;
        if entry.size() > MAX_RESOURCE {
            return Err(Error::Budget(path.into()));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.take(MAX_RESOURCE + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_RESOURCE {
            return Err(Error::Budget(path.into()));
        }
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_archive_error_identifies_the_runtime_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("client.jar");
        let error = Jar::open(&path, "unused", 0).err().unwrap();
        assert!(error.to_string().contains(&path.display().to_string()));
        assert!(matches!(error, Error::Io(e) if e.kind() == std::io::ErrorKind::NotFound));
    }
    #[test]
    fn namespaces_and_archive_paths_cannot_escape_assets() {
        let id = Identifier::parse("example:block/stone").unwrap();
        assert_eq!(
            id.asset_path("models", "json"),
            "assets/example/models/block/stone.json"
        );
        assert_eq!(
            Identifier::parse("block/stone").unwrap().namespace(),
            "minecraft"
        );
        for invalid in [
            "../secret",
            "/absolute",
            "a//b",
            "minecraft:..",
            "UPPER:path",
            "a:b:c",
            "a\\b",
            ":empty",
            "empty:",
        ] {
            assert!(Identifier::parse(invalid).is_err(), "{invalid}");
        }
    }
}
