//! Java credentials use the same protected files as account storage, in a separate namespace.
use crate::accounts::{private_directory, read_private, write_private};
use java_protocol::auth::Account;
use std::{io, path::Path};
/// Persists a signed-in account before selecting it. No Bedrock cache is read.
pub fn save(root: &Path, account: &Account) -> io::Result<()> {
    private_directory(root)?;
    let _lease = crate::accounts::try_lease(&root.join("accounts.lock"))?;
    let id = account.profile.id.to_string();
    write_private(
        &root.join(format!("{id}.json")),
        &serde_json::to_vec(account).map_err(io::Error::other)?,
    )?;
    write_private(&root.join("active"), id.as_bytes())
}
pub fn load_id(root: &Path, id: uuid::Uuid) -> io::Result<Account> {
    let account: Account = serde_json::from_slice(&read_private(&root.join(format!("{id}.json")))?)
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
    if active_id(root).is_ok_and(|selected| selected == id) {
        std::fs::remove_file(root.join("active"))?;
    }
    std::fs::remove_file(root.join(format!("{id}.json")))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(id: uuid::Uuid) -> Account {
        serde_json::from_value(serde_json::json!({"profile":{"id":id,"name":"Fixture"},"access_token":"test-only","refresh_token":"test-only","expires_at":0,"client_id":"00000000-0000-4000-8000-000000000001"})).unwrap()
    }
    #[test]
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
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Refresh cannot resurrect a logged-out account or undo a concurrent account selection.
pub fn save_refreshed(root: &Path, account: &Account) -> io::Result<()> {
    let _lease = crate::accounts::try_lease(&root.join("accounts.lock"))?;
    if active_id(root)? != account.profile.id {
        return Err(io::Error::other("Java account changed during refresh"));
    }
    write_private(
        &root.join(format!("{}.json", account.profile.id)),
        &serde_json::to_vec(account).map_err(io::Error::other)?,
    )
}
