//! Generated packet identities for exactly one pinned release; see data/target.json.
include!(concat!(env!("OUT_DIR"), "/target.rs"));

/// Looks up a packet identity in the pinned state table, never across states.
pub fn id(names: &[&str], name: &str) -> i32 {
    names
        .iter()
        .position(|entry| *entry == name)
        .expect("packet belongs to pinned target") as i32
}
