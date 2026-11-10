//! Ordinary placement feedback. Collision certainty remains separate from a cosmetic
//! attempt gesture; neither changes blocks, stacks or server permission.
use assets::registry::java::physics::{Bounds, Collision, Table};
use client_world::java::WorldState;
use gameplay::java::interaction::{Request, Target};
use sim::java::{selection::Face, travel::Body};

#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub physics: &'a Table,
    pub body: Body,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Clear,
    Blocked,
    UnknownEntities,
}
impl Outcome {
    /// Known obstructions suppress feedback; unresolved entity data is an attempt only.
    pub(super) fn gesture(self) -> bool {
        self != Self::Blocked
    }
}

pub(super) fn preview(
    state: &WorldState,
    request: &Request,
    context: Context<'_>,
) -> Option<Outcome> {
    eligible(state, request, context)
}
/// Returns exact native default-state placement facts only after all existing preview gates pass.
pub(super) fn candidate(
    state: &WorldState,
    request: &Request,
    context: Context<'_>,
) -> Option<([i32; 3], u32)> {
    if preview(state, request, context) != Some(Outcome::Clear) {
        return None;
    }
    let Target::Block(hit) = request.target else {
        return None;
    };
    let stack = request.hand.held(&state.inventory)?.as_ref()?;
    let rules = assets::registry::java::use_rules::builtin().ok()?;
    let block = rules.item(&stack.item)?.placement_block.as_deref()?;
    Some((
        offset(hit.block, hit.face)?,
        context
            .physics
            .registry()?
            .state_id(block, &Default::default())?,
    ))
}
#[cfg(test)]
fn predict(state: &WorldState, request: &Request, context: Context<'_>) -> bool {
    preview(state, request, context) == Some(Outcome::Clear)
}
fn eligible(state: &WorldState, request: &Request, context: Context<'_>) -> Option<Outcome> {
    use world::java::hud::GameMode;
    if !matches!(
        state.status.mode,
        Some(GameMode::Survival | GameMode::Creative)
    ) {
        return None;
    }
    let dimension = state.terrain.dimension()?;
    if dimension.epoch != request.dimension_epoch
        || state.motion_sequence() != request.correction_sequence
    {
        return None;
    }
    let Target::Block(hit) = request.target else {
        return None;
    };
    if state.terrain.block(hit.block)?.0 != hit.state {
        return None;
    }
    let held = request.hand.held(&state.inventory)?;
    let stack = held.as_ref()?;
    if stack.count == 0 || stack.added.contains_key("minecraft:block_state") {
        return None;
    }
    let id = java_protocol::target::ITEM_TYPES
        .iter()
        .position(|n| *n == stack.item)?;
    let defaults = java_protocol::target::ITEM_USE_DEFAULTS[id];
    if !gameplay::java::item_use::instant(
        Some(held),
        Some(gameplay::java::item_use::Defaults {
            duration: defaults.duration,
            group: defaults.group,
        }),
        &state.inventory.cooldowns,
    )
    .ok()?
    {
        return Some(Outcome::Blocked);
    }
    let rules = assets::registry::java::use_rules::builtin().ok()?;
    let target = rules.block(state.block_name(hit.state)?)?;
    // Non-base dispatch (e.g. opening a chest) and replaceable targets retain
    // their existing request path, without inventing an ordinary placement result.
    if !target.item_default || !target.empty_default || !target.placement_surface {
        return None;
    }
    let block = rules.item(&stack.item)?.placement_block.as_deref()?;
    if !rules.block(block)?.placement_default {
        return None;
    }
    let destination = offset(hit.block, hit.face)?;
    let scope = (dimension.session, dimension.epoch);
    if destination[1] < dimension.min_y
        || destination[1] >= dimension.min_y.checked_add(dimension.height)?
        || !state.border.contains_block(scope, hit.block).ok()?
        || !state.border.contains_block(scope, destination).ok()?
    {
        return Some(Outcome::Blocked);
    }
    let existing = context.physics.state(state.terrain.block(destination)?.0)?;
    if !existing.is_air || !existing.fluid_empty {
        return None;
    }
    let registry = context.physics.registry()?;
    let placed = registry.state_id(block, &Default::default())?;
    let Collision::Static(boxes) = &context.physics.state(placed)?.collision else {
        return None;
    };
    if boxes.len() != 1 || boxes[0].min != [0.; 3] || boxes[0].max != [1.; 3] {
        return None;
    }
    let occupied = Bounds {
        min: destination.map(f64::from),
        max: destination.map(|n| f64::from(n) + 1.),
    };
    let scale = state
        .player_dynamics
        .attributes
        .get("minecraft:scale")?
        .value()
        .ok()? as f32;
    if overlaps(
        occupied,
        context.body.bounds(state.player_motion?.position, scale),
    ) {
        return Some(Outcome::Blocked);
    }
    let mut unknown = false;
    for entity in state.entities.iter() {
        if rules.never_obstructs(&entity.kind)
            || state.player_entity_mode(entity.network_id) == Some(GameMode::Spectator)
            || entity.visual.armor_stand_flags.is_some_and(|f| f & 16 != 0)
        {
            continue;
        }
        let bounds = (|| {
            // Unknown collision semantics remain unpredicted, not replaced with a
            // guessed box. They do not prevent sending the authoritative request.
            if entity.visual.incomplete.is_some()
                || state.entities.passengers(entity.network_id).is_some()
            {
                return None;
            }
            let position = entity.presentation.as_ref()?.track.current.position;
            let bounds = if entity.visual.player.is_some() {
                match entity.visual.pose.as_ref() {
                    "standing" => sim::java::travel::bounds(position, false, entity.visual.scale),
                    "crouching" => sim::java::travel::bounds(position, true, entity.visual.scale),
                    _ => return None,
                }
            } else {
                if (entity.visual.pig.is_none()
                    && entity.visual.villager.is_none()
                    && entity.visual.armor_stand_flags.is_none())
                    || entity.visual.pose.as_ref() != "standing"
                {
                    return None;
                }
                let id = java_protocol::target::ENTITY_TYPES
                    .iter()
                    .position(|n| *n == entity.kind.as_ref())?;
                let dims = java_protocol::target::ENTITY_SPECS[id].dimensions;
                let small = entity.visual.baby
                    || entity.visual.armor_stand_flags.is_some_and(|f| f & 1 != 0);
                let age = if small { 0.5_f32 } else { 1. };
                let width = f64::from((dims[0] * age) * entity.visual.scale);
                let height = f64::from((dims[1] * age) * entity.visual.scale);
                Bounds {
                    min: [
                        position[0] - width / 2.,
                        position[1],
                        position[2] - width / 2.,
                    ],
                    max: [
                        position[0] + width / 2.,
                        position[1] + height,
                        position[2] + width / 2.,
                    ],
                }
            };
            Some(bounds)
        })();
        let Some(bounds) = bounds else {
            unknown = true;
            continue;
        };
        if overlaps(occupied, bounds) {
            return Some(Outcome::Blocked);
        }
    }
    Some(if unknown {
        Outcome::UnknownEntities
    } else {
        Outcome::Clear
    })
}
fn overlaps(a: Bounds, b: Bounds) -> bool {
    (0..3).all(|i| a.min[i] < b.max[i] && a.max[i] > b.min[i])
}
fn offset(mut p: [i32; 3], face: Face) -> Option<[i32; 3]> {
    let (axis, delta) = match face {
        Face::Down => (1, -1),
        Face::Up => (1, 1),
        Face::North => (2, -1),
        Face::South => (2, 1),
        Face::West => (0, -1),
        Face::East => (0, 1),
    };
    p[axis] = p[axis].checked_add(delta)?;
    Some(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (WorldState, Table, Request) {
        use sha2::{Digest, Sha256};
        use std::sync::Arc;
        use world::java::{BiomeId, BlockStateId, Column, Dimension, Light, Palette, Section};
        // Keep the exercised air/stone/wool state IDs native. Unread filler states
        // only make the synthetic registry contiguous; they supply no prediction facts.
        let facts = assets::registry::java::prediction::builtin().unwrap();
        let wool = (0..java_protocol::target::BLOCK_STATE_COUNT as u32)
            .find(|id| facts.state(*id).unwrap().0.name == "minecraft:white_wool")
            .unwrap();
        let fillers: Vec<_> = (2..wool)
            .map(|id| serde_json::json!({"id":id,"default":id==2}))
            .collect();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "minecraft:air":{"states":[{"id":0,"default":true}]},
            "minecraft:stone":{"states":[{"id":1,"default":true}]},
            "minecraft:fixture_unused":{"states":fillers},
            "minecraft:white_wool":{"states":[{"id":wool,"default":true}]}
        }))
        .unwrap();
        let registry = Arc::new(
            assets::registry::java::BlockRegistry::load(
                &bytes,
                &format!("{:x}", Sha256::digest(&bytes)),
            )
            .unwrap(),
        );
        let mut wire = b"CBJPHYS1".to_vec();
        wire.extend((wool + 1).to_be_bytes());
        for id in 0_u32..=wool {
            wire.extend(id.to_be_bytes());
            wire.push(0);
            for n in [0.6_f32, 1., 1., 0.] {
                wire.extend(n.to_be_bytes());
            }
            wire.extend([u8::from(id == 0), 1]);
            let fluid = b"minecraft:empty";
            wire.extend((fluid.len() as u16).to_be_bytes());
            wire.extend(fluid);
            wire.extend(u32::from(id != 0).to_be_bytes());
            if id != 0 {
                for n in [0_f64, 0., 0., 1., 1., 1.] {
                    wire.extend(n.to_be_bytes());
                }
            }
        }
        let physics = Table::load(
            &wire,
            &format!("{:x}", Sha256::digest(&wire)),
            wool as usize + 1,
        )
        .unwrap()
        .with_registry(registry.clone())
        .unwrap();
        let mut state = WorldState::default();
        state.begin(7);
        state.status.mode = Some(world::java::hud::GameMode::Survival);
        state.terrain.begin(Dimension {
            session: 7,
            epoch: 1,
            name: "minecraft:overworld".into(),
            min_y: -64,
            height: 16,
            has_skylight: true,
        });
        state.install_block_registry(registry).unwrap();
        for x in [-2, -1, 0] {
            assert!(state.terrain.replace(
                [x, 0],
                Column::new(
                    vec![Arc::new(Section {
                        blocks: Palette::uniform(BlockStateId(0), 4096),
                        biomes: Palette::uniform(BiomeId(0), 64)
                    })],
                    Light {
                        sky: vec![None; 3],
                        block: vec![None; 3]
                    },
                    vec![],
                    vec![]
                )
            ));
        }
        state.terrain.set_block([-16, -60, 1], BlockStateId(1));
        state.inventory.begin(7, 1);
        state
            .inventory
            .admit(
                7,
                1,
                inventory::java::Update::Slot {
                    container: 0,
                    state: 1,
                    slot: 36,
                    stack: Some(inventory::java::Stack {
                        item: "minecraft:white_wool".into(),
                        count: 4,
                        added: Default::default(),
                        removed: Default::default(),
                    }),
                },
            )
            .unwrap();
        state.player_dynamics.attributes.insert(
            "minecraft:scale".into(),
            world::java::dynamics::Attribute {
                base: 1.,
                minimum: 0.0625,
                maximum: 16.,
                modifiers: vec![],
            },
        );
        state.player_motion = Some(world::java::player::Motion {
            position: [-18., -59., 1.],
            ..Default::default()
        });
        let request = Request {
            hand: gameplay::java::item_use::plan::Hand::Main,
            kind: gameplay::java::interaction::Kind::UseOn,
            dimension_epoch: 1,
            correction_sequence: state.motion_sequence(),
            sequence: 1,
            target: Target::Block(sim::java::selection::Hit {
                block: [-16, -60, 1],
                state: 1,
                position: [-15.5, -59., 1.5],
                face: Face::Up,
                inside: false,
            }),
        };
        (state, physics, request)
    }
    #[test]
    fn native_direct_prediction_mutates_creative_placement_and_completed_break_only() {
        use super::super::prediction::apply;
        use gameplay::java::interaction::Kind;
        use world::java::{BlockStateId, dynamics::Abilities, hud::GameMode};
        let (mut state, physics, mut request) = fixture();
        let context = Some(Context {
            physics: &physics,
            body: Body::default(),
        });
        assert!(
            !apply(&mut state, &request, false, context),
            "survival placement awaits native inventory consumption"
        );
        state.status.mode = Some(GameMode::Creative);
        state.player_dynamics.abilities = Some(Abilities {
            invulnerable: true,
            flying: false,
            may_fly: true,
            creative: true,
            flying_speed: 0.05,
            walking_speed: 0.1,
        });
        let destination = [-16, -59, 1];
        let wool = physics
            .registry()
            .unwrap()
            .state_id("minecraft:white_wool", &Default::default())
            .unwrap();
        assert!(apply(&mut state, &request, false, context));
        assert_eq!(state.terrain.block(destination), Some(BlockStateId(wool)));
        assert_eq!(
            state.block_updates, 0,
            "local prediction is not a server update"
        );
        assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 4);
        assert!(
            state.terrain.reconcile_blocks(1)[0].changed,
            "ACK without server write restores air"
        );
        assert_eq!(state.terrain.block(destination), Some(BlockStateId(0)));
        request.sequence = 2;
        request.kind = Kind::Break;
        state.status.mode = Some(GameMode::Survival);
        assert!(
            !apply(&mut state, &request, false, context),
            "survival START is not completion"
        );
        request.kind = Kind::AbortBreak;
        assert!(!apply(&mut state, &request, true, context));
        request.kind = Kind::FinishBreak;
        assert!(apply(&mut state, &request, true, context));
        assert_eq!(state.terrain.block([-16, -60, 1]), Some(BlockStateId(0)));
        assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 4);
        assert!(state.terrain.reconcile_blocks(2)[0].changed);
        assert_eq!(state.terrain.block([-16, -60, 1]), Some(BlockStateId(1)));
        let facts = assets::registry::java::prediction::builtin().unwrap();
        let grass = (0..java_protocol::target::BLOCK_STATE_COUNT as u32)
            .find(|id| facts.state(*id).unwrap().0.name == "minecraft:grass_block")
            .unwrap();
        state.terrain.set_block([-15, -60, 1], BlockStateId(grass));
        request.sequence = 3;
        assert!(
            !apply(&mut state, &request, true, context),
            "SnowyBlock.updateShape is not an inert neighbor callback"
        );
    }
    #[test]
    fn ordinary_placement_predicts_only_the_gesture_with_current_known_authority() {
        let (mut state, physics, mut request) = fixture();
        let context = Context {
            physics: &physics,
            body: Body::default(),
        };
        assert!(predict(&state, &request, context));
        request.hand = gameplay::java::item_use::plan::Hand::Off;
        assert!(
            !predict(&state, &request, context),
            "unknown offhand is not empty or a usable stack"
        );
        request.hand = gameplay::java::item_use::plan::Hand::Main;
        assert_eq!(state.terrain.block([-16, -59, 1]).unwrap().0, 0);
        assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 4);
        state.player_motion.as_mut().unwrap().position = [-15.5, -59., 1.5];
        assert!(!predict(&state, &request, context));
        state.player_motion.as_mut().unwrap().position = [-18., -59., 1.];
        state
            .terrain
            .set_block([-16, -59, 1], world::java::BlockStateId(1));
        assert!(!predict(&state, &request, context));
        state
            .terrain
            .set_block([-16, -59, 1], world::java::BlockStateId(0));
        state
            .inventory
            .cooldowns
            .set("minecraft:white_wool".into(), 4)
            .unwrap();
        assert!(!predict(&state, &request, context));
        state
            .inventory
            .cooldowns
            .set("minecraft:white_wool".into(), 0)
            .unwrap();
        assert!(predict(&state, &request, context));
        request.dimension_epoch = 2;
        assert!(!predict(&state, &request, context));
        request.dimension_epoch = 1;
        request.correction_sequence += 1;
        assert!(!predict(&state, &request, context));
        request.correction_sequence -= 1;
        state.terrain.unload([-1, 0]);
        assert!(!predict(&state, &request, context));
    }
    #[test]
    fn unknown_entities_keep_collision_uncertain_and_known_obstructions_still_win() {
        use world::java::entities::{Spawn, visual::Visual};
        let (mut state, physics, request) = fixture();
        let context = Context {
            physics: &physics,
            body: Body::default(),
        };
        state.entities.begin(7, 1);
        let spawn = |id: i32, kind: &str, position: [f64; 3], stand: bool| Spawn {
            transport_lifetime: id as u64,
            network_id: id,
            uuid: [id as u8; 16],
            kind: kind.into(),
            motion: world::java::player::Motion {
                position,
                ..Default::default()
            },
            head_yaw: 0.,
            object_data: 0,
            visual: Visual {
                armor_stand_flags: stand.then_some(0),
                ..Default::default()
            },
            interpolation_interval: stand.then_some(3),
        };
        state
            .entities
            .spawn((7, 1), spawn(98, "minecraft:cow", [20., -59., 20.], false))
            .unwrap();
        assert_eq!(
            preview(&state, &request, context),
            Some(Outcome::UnknownEntities)
        );
        assert!(
            !predict(&state, &request, context),
            "unknown is never a successful collision preview"
        );
        assert!(
            Outcome::UnknownEntities.gesture(),
            "feedback describes only an attempt"
        );
        state
            .entities
            .spawn(
                (7, 1),
                spawn(
                    99,
                    java_protocol::target::ENTITY_ARMOR_STAND,
                    [-15.5, -59., 1.5],
                    true,
                ),
            )
            .unwrap();
        assert_eq!(
            preview(&state, &request, context),
            Some(Outcome::Blocked),
            "an earlier unknown cannot hide a known overlap"
        );
        assert!(!Outcome::Blocked.gesture());
        state.entities.remove((7, 1), &[99]).unwrap();
        state.player_motion.as_mut().unwrap().position = [-15.5, -59., 1.5];
        assert_eq!(preview(&state, &request, context), Some(Outcome::Blocked));
        assert_eq!(state.terrain.block([-16, -59, 1]).unwrap().0, 0);
        assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 4);
    }
    #[test]
    fn an_unremoved_dead_body_still_obstructs_but_a_marker_does_not() {
        use world::java::entities::{Spawn, visual::Visual};
        let (mut state, physics, request) = fixture();
        let context = Context {
            physics: &physics,
            body: Body::default(),
        };
        state.entities.begin(7, 1);
        state
            .entities
            .spawn(
                (7, 1),
                Spawn {
                    transport_lifetime: 1,
                    network_id: 99,
                    uuid: [1; 16],
                    kind: java_protocol::target::ENTITY_ARMOR_STAND.into(),
                    motion: world::java::player::Motion {
                        position: [-15.5, -59., 1.5],
                        ..Default::default()
                    },
                    head_yaw: 0.,
                    object_data: 0,
                    visual: Visual {
                        armor_stand_flags: Some(0),
                        health: Some(0.),
                        ..Default::default()
                    },
                    interpolation_interval: Some(3),
                },
            )
            .unwrap();
        // EntityGetter.isUnobstructed tests isRemoved, not isAlive/health.
        assert!(!predict(&state, &request, context));
        state
            .entities
            .update((7, 1), 99, |e| {
                e.visual.armor_stand_flags = Some(16);
                Ok(())
            })
            .unwrap();
        assert!(predict(&state, &request, context));
        state.entities.remove((7, 1), &[99]).unwrap();
        assert!(predict(&state, &request, context));
    }
    #[test]
    fn native_placement_faces_negative_edges_and_contact_do_not_overlap() {
        assert_eq!(offset([-16, -60, -1], Face::Up), Some([-16, -59, -1]));
        assert_eq!(offset([i32::MAX, 0, 0], Face::East), None);
        let b = Bounds {
            min: [-16., -59., -1.],
            max: [-15., -58., 0.],
        };
        assert!(!overlaps(
            b,
            Bounds {
                min: [-15., -59., -1.],
                max: [-14., -58., 0.]
            }
        ));
        assert!(overlaps(
            b,
            Bounds {
                min: [-15.1, -59., -1.],
                max: [-14., -58., 0.]
            }
        ));
        let rules = assets::registry::java::use_rules::builtin().unwrap();
        assert!(rules.never_obstructs("minecraft:item"));
        assert!(!rules.never_obstructs("minecraft:player"));
        assert!(!rules.never_obstructs("minecraft:armor_stand"));
        assert!(!rules.never_obstructs("unknown:entity"));
    }
}
