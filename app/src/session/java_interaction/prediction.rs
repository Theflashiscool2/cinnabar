//! Direct-cell native prediction gated by pinned callback owners and existing interaction intent.
use super::placement;
use assets::registry::java::{
    physics::{Collision, Table},
    prediction::Table as Facts,
};
use client_world::java::WorldState;
use gameplay::java::interaction::{Kind, Request, Target};
use world::java::{BlockStateId, hud::GameMode};

/// Applies only verified direct mutations; unimplemented callbacks remain server-owned.
pub(super) fn apply(
    state: &mut WorldState,
    request: &Request,
    completed_break: bool,
    context: Option<placement::Context<'_>>,
) -> bool {
    let Some(context) = context else {
        return false;
    };
    let Some((position, block)) = mutation(state, request, completed_break, context) else {
        return false;
    };
    let Some(motion) = state.player_motion else {
        return false;
    };
    state.terrain.predict_block(
        request.sequence,
        position,
        BlockStateId(block),
        motion.position,
    )
}
fn mutation(
    state: &WorldState,
    request: &Request,
    completed_break: bool,
    context: placement::Context<'_>,
) -> Option<([i32; 3], u32)> {
    let Target::Block(hit) = request.target else {
        return None;
    };
    if request.sequence <= 0
        || request.correction_sequence != state.motion_sequence()
        || state.terrain.dimension()?.epoch != request.dimension_epoch
        || state.terrain.block(hit.block)?.0 != hit.state
    {
        return None;
    }
    let facts = assets::registry::java::prediction::builtin().ok()?;
    let (position, block) = match request.kind {
        Kind::UseOn => {
            // Native Creative restores its count after useOn. Survival stack consumption
            // must first join the existing inventory working view before predicting placement.
            if state.status.mode != Some(GameMode::Creative)
                || !state.player_dynamics.abilities.as_ref()?.creative
            {
                return None;
            }
            let (position, block) = placement::candidate(state, request, context)?;
            if !facts.state(block)?.0.direct_placement() {
                return None;
            }
            (position, block)
        }
        Kind::Break | Kind::FinishBreak if completed_break => {
            if !matches!(
                state.status.mode,
                Some(GameMode::Creative | GameMode::Survival)
            ) {
                return None;
            }
            if let Some(stack) = state.inventory.held()?.as_ref()
                && !facts.base_item_destruction(&stack.item)
            {
                return None;
            }
            let (target, legacy) = facts.state(hit.state)?;
            if !target.direct_destruction() || context.physics.state(hit.state)?.is_air {
                return None;
            }
            // This is the actual native fluid-derived state, never unconditional AIR.
            // A fluid/onPlace override remains unimplemented rather than guessed inert.
            if !facts.state(legacy)?.0.direct_placement() {
                return None;
            }
            (hit.block, legacy)
        }
        _ => return None,
    };
    let dimension = state.terrain.dimension()?;
    if !state
        .border
        .contains_block((dimension.session, dimension.epoch), position)
        .ok()?
        || !direct_neighbours(state, context.physics, facts, position)
    {
        return None;
    }
    let before = state.terrain.block(position)?.0;
    if !facts.state(before)?.0.inert_neighbour()
        || !matches!(
            context.physics.state(before)?.collision,
            Collision::Static(_)
        )
        || !matches!(
            context.physics.state(block)?.collision,
            Collision::Static(_)
        )
    {
        return None;
    }
    Some((position, block))
}
fn direct_neighbours(
    state: &WorldState,
    physics: &Table,
    facts: &Facts,
    position: [i32; 3],
) -> bool {
    (0..3).all(|axis| {
        [-1, 1].into_iter().all(|delta| {
            let mut cell = position;
            let Some(value) = cell[axis].checked_add(delta) else {
                return false;
            };
            cell[axis] = value;
            let Some(id) = state.terrain.block(cell).map(|id| id.0) else {
                return false;
            };
            facts
                .state(id)
                .is_some_and(|(block, _)| block.inert_neighbour())
                && physics
                    .state(id)
                    .is_some_and(|state| matches!(state.collision, Collision::Static(_)))
        })
    })
}
