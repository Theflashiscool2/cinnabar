//! Credential bytes stay behind this boundary; iOS never falls back to a file.
use std::{io, path::Path};

#[cfg(target_os = "ios")]
mod keychain;

pub(super) fn read(root: &Path, id: uuid::Uuid) -> io::Result<Vec<u8>> {
    #[cfg(target_os = "ios")]
    {
        let _ = root;
        keychain::read(&id.to_string())
    }
    #[cfg(not(target_os = "ios"))]
    {
        crate::accounts::read_private(&root.join(format!("{id}.json")))
    }
}
pub(super) fn write(root: &Path, id: uuid::Uuid, bytes: &[u8]) -> io::Result<()> {
    #[cfg(target_os = "ios")]
    {
        let _ = root;
        keychain::write(&id.to_string(), bytes)
    }
    #[cfg(not(target_os = "ios"))]
    {
        crate::accounts::write_private(&root.join(format!("{id}.json")), bytes)
    }
}
pub(super) fn delete(root: &Path, id: uuid::Uuid) -> io::Result<()> {
    #[cfg(target_os = "ios")]
    {
        let _ = root;
        keychain::delete(&id.to_string())
    }
    #[cfg(not(target_os = "ios"))]
    {
        std::fs::remove_file(root.join(format!("{id}.json")))
    }
}
