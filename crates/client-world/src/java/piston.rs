//! Binds compile-time native facts to the edition-independent asset owner.
use assets::registry::java::piston::Table;
use std::sync::OnceLock;
mod events;
use assets::registry::java::{BlockRegistry, piston::Kind};
pub(super) use events::event;
use world::java::{
    BlockStateId, Terrain,
    piston::{Motion, Moving, Snapshot},
};

pub(super) fn initialize(
    t: &mut Terrain,
    registry: &BlockRegistry,
    chunk: Option<[i32; 2]>,
) -> Result<(), &'static str> {
    let table = facts()?;
    let entities: Vec<_> = t
        .columns()
        .filter(|(key, _)| chunk.is_none_or(|c| c == *key))
        .flat_map(|(_, c)| c.block_entities.iter())
        .filter(|e| {
            t.block(e.position)
                .and_then(|id| table.variant(id.0))
                .is_some_and(|v| v.kind == Kind::Moving)
        })
        .cloned()
        .collect();
    for entity in entities {
        if t.pistons.contains_key(&entity.position)
            || !t
                .block(entity.position)
                .and_then(|id| table.variant(id.0))
                .is_some_and(|v| v.kind == Kind::Moving)
        {
            continue;
        }
        let Some(data) = entity.data.as_ref() else {
            continue;
        };
        let snapshot = Snapshot::read(data)?;
        let state = registry
            .state_id(&snapshot.name, &snapshot.properties)
            .ok_or("unresolved Java piston snapshot state")?;
        let generation = t
            .block_generation(entity.position)
            .ok_or("nonresident Java piston snapshot")?;
        t.pistons.insert(
            entity.position,
            Moving {
                state,
                facing: snapshot.facing,
                extending: snapshot.extending,
                source: snapshot.source,
                motion: Motion::new(snapshot.progress)?,
                generation,
            },
        );
    }
    Ok(())
}
pub(super) fn tick(t: &mut Terrain, registry: Option<&BlockRegistry>) -> Result<(), &'static str> {
    // A late authoritative chunk is a post-event snapshot, not the event's original view.
    // Never replay a push against it: that could move an already-moved block a second time.
    let retired: Vec<_> = t
        .pending_pistons
        .iter()
        .filter_map(|(p, event)| {
            event
                .dependencies
                .iter()
                .any(|(q, observed)| t.block_generation(*q) != *observed)
                .then_some(*p)
        })
        .collect();
    for p in retired {
        t.pending_pistons.remove(&p);
    }
    let mut finished = Vec::new();
    for (p, m) in &mut t.pistons {
        m.motion.tick();
        if m.motion.retired() {
            let Some(registry) = registry else { continue };
            let state = registry.state(m.state).ok_or("invalid Java moved block")?;
            let mut props = state.properties.clone();
            if props.get("waterlogged").is_some_and(|s| s == "true") {
                props.insert("waterlogged".into(), "false".into());
            }
            let id = registry
                .state_id(&state.name, &props)
                .ok_or("unresolved Java piston final state")?;
            finished.push((*p, id, m.generation));
        }
    }
    for (p, id, generation) in finished {
        let current = t.block_generation(p) == Some(generation);
        t.pistons.remove(&p);
        if current {
            t.set_block(p, BlockStateId(id));
        }
    }
    Ok(())
}

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
        assert_eq!(table.variants().len(), 60);
        for (kind, expected) in [(Kind::Base, 24), (Kind::Head, 24), (Kind::Moving, 12)] {
            assert_eq!(
                table.variants().iter().filter(|v| v.kind == kind).count(),
                expected
            );
        }
        let ids: std::collections::BTreeSet<_> = table.variants().iter().map(|v| v.id).collect();
        assert_eq!(ids.len(), 60);
    }
}
