//! Separate Java profiles and credentials: iOS Keychain, private desktop cache.
use crate::accounts::{private_directory, read_private, write_private};
use java_protocol::auth::Account;
use std::{io, path::Path};
mod credentials;

/// Public metadata, kept separate from reusable credentials.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    pub profile: java_protocol::auth::Profile,
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Add,
    Select(usize),
    Refresh,
    Remove,
    OpenLink,
    Cancel,
    Continue,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub open: bool,
    pub pending_server: Option<String>,
    pub accounts: Vec<Summary>,
    pub labels: Vec<String>,
    pub selected: Option<uuid::Uuid>,
    pub busy: bool,
    pub ready: bool,
    pub message: String,
    pub prompt: Option<java_protocol::auth::DevicePrompt>,
}

fn metadata(root: &Path, account: &Account) -> io::Result<()> {
    write_private(
        &root.join(format!("{}.profile.json", account.profile.id)),
        &serde_json::to_vec(&Summary {
            profile: account.profile.clone(),
            expires_at: account.expires_at(),
        })
        .map_err(io::Error::other)?,
    )
}

/// Enumerates only nonsecret profile sidecars, never token files.
pub fn list(root: &Path) -> io::Result<Vec<Summary>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut accounts = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(id) = name
            .strip_suffix(".profile.json")
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
        else {
            continue;
        };
        let account: Summary = serde_json::from_slice(&read_private(&entry.path())?)
            .map_err(|_| io::Error::other("Invalid Java account metadata"))?;
        if account.profile.id != id {
            return Err(io::Error::other("Java account metadata identity mismatch"));
        }
        accounts.push(account);
    }
    accounts.sort_by(|a, b| {
        a.profile
            .name
            .cmp(&b.profile.name)
            .then(a.profile.id.cmp(&b.profile.id))
    });
    Ok(accounts)
}
/// Persists a signed-in account before selecting it. No Bedrock cache is read.
pub fn save(root: &Path, account: &Account) -> io::Result<()> {
    private_directory(root)?;
    let _lease = crate::accounts::try_lease(&root.join("accounts.lock"))?;
    let id = account.profile.id.to_string();
    credentials::write(
        root,
        account.profile.id,
        &serde_json::to_vec(account).map_err(io::Error::other)?,
    )?;
    metadata(root, account)?;
    write_private(&root.join("active"), id.as_bytes())
}
pub fn load_id(root: &Path, id: uuid::Uuid) -> io::Result<Account> {
    let account: Account = serde_json::from_slice(&credentials::read(root, id)?)
        .map_err(|_| io::Error::other("Invalid Java credential cache"))?;
    if account.profile.id != id {
        return Err(io::Error::other("Java account identity mismatch"));
    }
    Ok(account)
}
pub fn active_id(root: &Path) -> io::Result<uuid::Uuid> {
    let bytes = read_private(&root.join("active"))?;
    let id = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .ok_or_else(|| io::Error::other("Invalid Java account selection"))?;
    Ok(id)
}
pub fn active(root: &Path) -> io::Result<Account> {
    load_id(root, active_id(root)?)
}
pub fn select(root: &Path, id: uuid::Uuid) -> io::Result<()> {
    let _lease = crate::accounts::try_lease(&root.join("accounts.lock"))?;
    let _ = load_id(root, id)?;
    write_private(&root.join("active"), id.to_string().as_bytes())
}
pub fn logout(root: &Path, id: uuid::Uuid) -> io::Result<()> {
    let _lease = crate::accounts::try_lease(&root.join("accounts.lock"))?;
    // Keep the selection and metadata available for retry if Keychain deletion fails.
    credentials::delete(root, id)?;
    if active_id(root).is_ok_and(|selected| selected == id) {
        std::fs::remove_file(root.join("active"))?;
    }
    match std::fs::remove_file(root.join(format!("{id}.profile.json"))) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(id: uuid::Uuid) -> Account {
        serde_json::from_value(serde_json::json!({"profile":{"id":id,"name":"Fixture"},"access_token":"test-only","refresh_token":"test-only","expires_at":0,"client_id":"00000000-0000-4000-8000-000000000001"})).unwrap()
    }
    #[test]
    #[cfg(not(target_os = "ios"))]
    fn private_cache_switch_logout_and_identity_validation() {
        let root = std::env::temp_dir().join(format!(
            "java-account-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = uuid::Uuid::from_u128(1);
        let second = uuid::Uuid::from_u128(2);
        save(&root, &fixture(first)).unwrap();
        save(&root, &fixture(second)).unwrap();
        save(&root, &fixture(second)).unwrap();
        assert_eq!(
            list(&root).unwrap().len(),
            2,
            "profile UUID deduplicates repeated authorization"
        );
        let public_metadata =
            std::fs::read_to_string(root.join(format!("{second}.profile.json"))).unwrap();
        assert!(!public_metadata.contains("test-only"));
        assert!(!public_metadata.contains("access_token"));
        assert!(!public_metadata.contains("refresh_token"));
        assert_eq!(active(&root).unwrap().profile.id, second);
        select(&root, first).unwrap();
        assert_eq!(active(&root).unwrap().profile.id, first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(root.join(format!("{first}.json")))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        logout(&root, first).unwrap();
        assert_eq!(list(&root).unwrap().len(), 1);
        assert!(active(&root).is_err());
        assert!(load_id(&root, first).is_err());
        assert!(save_refreshed(&root, &fixture(first)).is_err());
        select(&root, second).unwrap();
        assert_eq!(active(&root).unwrap().profile.id, second);
        assert!(save_refreshed(&root, &fixture(first)).is_err());
        assert_eq!(active_id(&root).unwrap(), second);
        std::fs::copy(
            root.join(format!("{second}.json")),
            root.join(format!("{first}.json")),
        )
        .unwrap();
        assert!(load_id(&root, first).is_err());
        // A credential backend deletion error must preserve metadata/selection for retry.
        let credential = root.join(format!("{second}.json"));
        std::fs::remove_file(&credential).unwrap();
        std::fs::create_dir(&credential).unwrap();
        assert!(logout(&root, second).is_err());
        assert_eq!(active_id(&root).unwrap(), second);
        assert!(
            list(&root)
                .unwrap()
                .iter()
                .any(|account| account.profile.id == second)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Refresh cannot resurrect a logged-out account or undo a concurrent account selection.
pub fn save_refreshed(root: &Path, account: &Account) -> io::Result<()> {
    let _lease = crate::accounts::try_lease(&root.join("accounts.lock"))?;
    if active_id(root)? != account.profile.id {
        return Err(io::Error::other("Java account changed during refresh"));
    }
    credentials::write(
        root,
        account.profile.id,
        &serde_json::to_vec(account).map_err(io::Error::other)?,
    )?;
    metadata(root, account)
}
