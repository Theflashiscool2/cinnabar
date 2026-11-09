//! Version-bound native callback identities; local movement never predicts server damage.
use crate::registry::java::BlockRegistry;
use serde::Deserialize;
use std::sync::OnceLock;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    BubbleColumn,
    Slime,
    ServerDamage,
    Other,
}
#[derive(Deserialize)]
struct Facts {
    schema: u32,
    game_version: String,
    protocol: u32,
    blocks_sha256: String,
    states: usize,
    bubble: Vec<String>,
    server_damage: Vec<String>,
    slime: Vec<String>,
    suppresses_bounce: String,
}
fn facts() -> &'static Facts {
    static FACTS: OnceLock<Facts> = OnceLock::new();
    FACTS.get_or_init(|| {
        let f: Facts =
            serde_json::from_str(include_str!("../../../../data/java-block-callbacks.json"))
                .expect("generated native callback facts");
        assert_eq!(f.schema, 1);
        assert!(!f.game_version.is_empty() && f.protocol > 0);
        f
    })
}
/// Exact native synchronized tag key used by Entity's collision restitution.
pub fn suppresses_bounce_tag() -> &'static str {
    &facts().suppresses_bounce
}
/// Class membership from the verified runtime, admitted only for its exact state registry.
pub fn kind(registry: &BlockRegistry, state: u32) -> Option<Kind> {
    let f = facts();
    if registry.len() != f.states || !registry.matches_sha256(&f.blocks_sha256) {
        return None;
    }
    let name = registry.state(state)?.name.as_ref();
    Some(if f.bubble.iter().any(|n| n == name) {
        Kind::BubbleColumn
    } else if f.slime.iter().any(|n| n == name) {
        Kind::Slime
    } else if f.server_damage.iter().any(|n| n == name) {
        Kind::ServerDamage
    } else {
        Kind::Other
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_callback_facts_match_the_supported_target() {
        let p: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../java-protocol/data/target.json"
        ))
        .unwrap();
        let f = facts();
        assert_eq!(f.game_version, p["game_version"]);
        assert_eq!(f.protocol, p["protocol"].as_u64().unwrap() as u32);
        assert_eq!(
            f.states,
            p["block_registry_report"]["states"].as_u64().unwrap() as usize
        );
        assert_eq!(f.blocks_sha256, p["block_registry_report"]["sha256"]);
        assert_eq!(f.bubble.len(), 1);
        assert_eq!(f.slime.len(), 1);
        assert_eq!(f.server_damage.len(), 1);
    }
}
