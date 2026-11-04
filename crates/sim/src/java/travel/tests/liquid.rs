use super::*;
use assets::registry::java::fluids;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use world::java::BlockStateId;

fn scene(height: f32, lava: bool) -> (Terrain, Table) {
    let (terrain, physics) = terrain::scene();
    let states: Vec<_> = (0..4).map(|id| serde_json::json!({
        "id":id,"kind":if id==2 { if lava { "lava" } else { "water" } } else { "empty" },
        "height":if id==2 {height}else{0.},"falling":false,"solid":id==1||id==3,
        "solid_render":id==1,"flow_blocking":id==1,"overlay":false,
        "invisible":id==0||id==2,"emission":0,"full_face":if id==1 {63}else{0},"occlusion":[0.,0.,0.,0.,0.,0.]
    })).collect();
    let bytes = serde_json::to_vec(&serde_json::json!({"schema":2,"states":states})).unwrap();
    let facts = fluids::Table::load(&bytes, &format!("{:x}", Sha256::digest(&bytes)), 4).unwrap();
    (terrain, physics.with_fluids(Arc::new(facts)).unwrap())
}

#[test]
fn source_water_travel_and_buoyancy_use_fluid_drag_not_land_friction() {
    let (mut terrain, physics) = scene(8_f32 / 9., false);
    for x in 6..11 {
        for z in 6..11 {
            terrain.set_block([x, 0, z], BlockStateId(2));
        }
    }
    let mut motion = Motion {
        position: [8.5, 0., 8.5],
        ..Default::default()
    };
    let mut body = Body::default();
    step(
        &mut motion,
        &mut body,
        Input {
            forward: 1.,
            jump: true,
            ..Default::default()
        },
        terrain::parameters(),
        &terrain,
        &physics,
        (7, 1),
    )
    .unwrap();
    assert_eq!(body.water_height, f64::from(8_f32 / 9.));
    assert_eq!(
        motion.position[2],
        8.5 + f64::from(0.98_f32) * f64::from(0.02_f32)
    );
    assert_eq!(motion.position[1], f64::from(0.04_f32));
    assert_eq!(
        motion.velocity[1],
        f64::from(0.04_f32) * f64::from(0.8_f32) - 0.005
    );
    assert!(!body.on_ground);
}

#[test]
fn shallow_and_deep_lava_have_distinct_vertical_drag() {
    for (height, expected) in [
        (1_f32 / 9., 0.1 * f64::from(0.8_f32) - 0.005 - 0.02),
        (8_f32 / 9., 0.030000000000000002),
    ] {
        let (mut terrain, physics) = scene(height, true);
        for x in 6..11 {
            for z in 6..11 {
                terrain.set_block([x, 0, z], BlockStateId(2));
            }
        }
        let mut motion = Motion {
            position: [8.5, 0., 8.5],
            velocity: [0., 0.1, 0.],
            ..Default::default()
        };
        step(
            &mut motion,
            &mut Body::default(),
            Input::default(),
            terrain::parameters(),
            &terrain,
            &physics,
            (7, 1),
        )
        .unwrap();
        assert_eq!(motion.velocity[1], expected);
    }
}

#[test]
fn current_uses_missing_neighbor_recovery_without_partial_prediction() {
    let (mut terrain, physics) = scene(8_f32 / 9., false);
    terrain.set_block([15, 0, 8], BlockStateId(2));
    let mut motion = Motion {
        position: [15.5, 0., 8.5],
        ..Default::default()
    };
    let before = motion;
    let mut body = Body::default();
    assert!(matches!(
        step(
            &mut motion,
            &mut body,
            Input::default(),
            terrain::parameters(),
            &terrain,
            &physics,
            (7, 1)
        ),
        Err(Error::Query(QueryError::Missing(_)))
    ));
    assert_eq!(motion, before);
    assert_eq!(body.water_height, 0.);
    let column = terrain.column([0, 0]).unwrap().as_ref().clone();
    terrain.replace([1, 0], column);
    step(
        &mut motion,
        &mut body,
        Input::default(),
        terrain::parameters(),
        &terrain,
        &physics,
        (7, 1),
    )
    .unwrap();
    assert!(body.water_height > 0.);
}

#[test]
fn water_surface_above_feet_and_current_minimum_are_not_full_cubes() {
    let (mut terrain, physics) = scene(1_f32 / 9., false);
    terrain.set_block([8, 0, 8], BlockStateId(2));
    let cells = super::super::liquid::Cells {
        terrain: &terrain,
        physics: &physics,
    };
    assert_eq!(
        cells
            .contact(bounds([8.5, 0.2, 8.5], false, 1.))
            .unwrap()
            .water,
        0.
    );
    let mut velocity = [0.; 3];
    super::super::liquid::push(&mut velocity, [0.1, 0., 0.], 0.014);
    assert_eq!(velocity, [0.0045000000000000005, 0., 0.]);
    // A moving player does not receive the stationary minimum-current boost.
    let mut velocity = [0.1, 0., 0.];
    super::super::liquid::push(&mut velocity, [0.1, 0., 0.], 0.014);
    assert_eq!(velocity, [0.1014, 0., 0.]);
}

#[test]
fn sneaking_keeps_edge_protection_while_standing_in_water() {
    let (mut terrain, physics) = scene(8_f32 / 9., false);
    for x in 6..12 {
        for z in 6..14 {
            terrain.set_block([x, 0, z], BlockStateId(2));
            if z >= 10 {
                terrain.set_block([x, -1, z], BlockStateId(0));
            }
        }
    }
    let mut motion = Motion {
        position: [8.5, 0., 9.9],
        velocity: [0., 0., 0.8],
        ..Default::default()
    };
    let mut body = Body {
        on_ground: true,
        ..Default::default()
    };
    step(
        &mut motion,
        &mut body,
        Input {
            forward: 1.,
            crouch: true,
            ..Default::default()
        },
        terrain::parameters(),
        &terrain,
        &physics,
        (7, 1),
    )
    .unwrap();
    // Native Player.maybeBackOffFromEdge has no dry-land predicate. Enough of
    // the body must remain above the supported side of the ledge, even when wet.
    assert!(
        motion.position[2] < 10.3,
        "wet sneak crossed the supported ledge: {:?}",
        motion.position
    );
}
