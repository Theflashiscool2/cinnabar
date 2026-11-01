//! Binds compile-time native facts to the edition-independent asset owner.
use assets::registry::java::piston::Table;
use std::sync::OnceLock;

/// No generated report or JDK is required on the player's device for these state predicates.
pub fn facts() -> Result<&'static Table, &'static str> {
    static FACTS: OnceLock<Result<Table, &'static str>> = OnceLock::new();
    FACTS
        .get_or_init(|| {
            Table::load(
                java_protocol::target::PISTON_BLOCK_FACTS_JSON,
                java_protocol::target::BLOCK_STATE_COUNT,
            )
        })
        .as_ref()
        .map_err(|e| *e)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_native_report_covers_every_state_and_no_other_ids() {
        let table = facts().unwrap();
        for id in 0..java_protocol::target::BLOCK_STATE_COUNT {
            assert!(
                table.state(id as u32).is_some(),
                "missing piston facts for state {id}"
            );
        }
        assert!(
            table
                .state(java_protocol::target::BLOCK_STATE_COUNT as u32)
                .is_none()
        );
        assert!(table.state(u32::MAX).is_none());
    }
}
