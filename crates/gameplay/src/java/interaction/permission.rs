//! Native block-action permission reads share the existing inventory component owner.
use super::Hand;
use assets::registry::java::adventure::Permission;
use client_world::java::WorldState;
use sim::java::selection::Hit;
use world::java::hud::GameMode;

/// Player.blockActionRestricted and ItemStack.useOn consult these components only
/// when mayBuild is false (Adventure); Spectator never admits block destruction.
pub fn check(
    state: &WorldState,
    hit: Hit,
    hand: Hand,
    permission: Permission,
) -> Result<bool, &'static str> {
    match state.status.mode {
        Some(GameMode::Survival | GameMode::Creative) => return Ok(true),
        Some(GameMode::Spectator) => return Ok(false),
        Some(GameMode::Adventure) => {}
        None => return Err("Native Adventure game mode is not known"),
    }
    let dimension = state
        .terrain
        .dimension()
        .ok_or("Native Adventure world is not known")?;
    if !state.inventory.accepts(dimension.session, dimension.epoch) {
        return Err("Native Adventure inventory scope is retired");
    }
    let current = state
        .terrain
        .block(hit.block)
        .ok_or("Native Adventure target terrain is not known")?
        .0;
    let block = state
        .block_state(current)
        .ok_or("Native Adventure state registry is not installed")?;
    let configuration = &state
        .bootstrap
        .as_ref()
        .ok_or("Native Adventure tags are not known")?
        .configuration;
    let facts = assets::registry::java::adventure::builtin()
        .map_err(|_| "Invalid pinned Adventure facts")?;
    let empty_tags = Default::default();
    let tags = (|| {
        let mut block_tags = None;
        for (name, tags) in &configuration.tags {
            if facts
                .identifier(name)
                .ok_or("Invalid native tag registry identifier")?
                .as_ref()
                != "minecraft:block"
            {
                continue;
            }
            if block_tags.is_some_and(|previous| previous != tags) {
                return Err(
                    "Native Adventure registry aliases require unavailable decoded wire ordering",
                );
            }
            block_tags = Some(tags);
        }
        Ok(block_tags.unwrap_or(&empty_tags))
    })();
    let hand = match hand {
        Hand::Main => inventory::java::BlockUseHand::Main,
        Hand::Off => inventory::java::BlockUseHand::Off,
    };
    state.inventory.adventure_permission(
        hand,
        permission,
        inventory::java::adventure::Target {
            state: current,
            block,
            tags,
            tag_revision: state.tag_revision,
        },
    )
}
