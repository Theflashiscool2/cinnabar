//! Edition-owned piston clocks and canonical block-model extraction for fractional placement.
use super::{SessionController, java_render};
use bevy::prelude::*;
use render_model::{block_entity::java_piston::Plan, posed_block::Vertex};
use render_scene::{ChunkTextureAssets, posed_block::PosedBlocks};
use std::{sync::Arc, time::Duration};
#[derive(Resource, Default)]
pub(crate) struct Clock {
    scope: Option<world::java::Dimension>,
    elapsed: Duration,
    replaced: std::collections::BTreeSet<[i32; 3]>,
}
pub(crate) fn advance(
    time: Res<Time<Real>>,
    mut clock: ResMut<Clock>,
    mut controller: ResMut<SessionController>,
) {
    let scope = controller
        .java
        .as_ref()
        .and(controller.java_world.terrain.dimension())
        .cloned();
    if clock.scope != scope {
        *clock = Clock {
            scope,
            ..Default::default()
        };
    }
    if clock.scope.is_none() {
        return;
    }
    clock.elapsed = clock.elapsed.saturating_add(time.delta());
    for _ in 0..simulation_clock::MAX_TICKS_PER_FRAME {
        if clock.elapsed < simulation_clock::TICK_DURATION {
            break;
        }
        clock.elapsed -= simulation_clock::TICK_DURATION;
        if let Err(error) = controller.java_world.tick_pistons() {
            controller.java_world.terrain_error = Some(error);
            break;
        }
    }
}
pub(crate) fn update(
    controller: Res<SessionController>,
    mut clock: ResMut<Clock>,
    mut presentation: ResMut<java_render::Presentation>,
    textures: Res<ChunkTextureAssets>,
    mut output: ResMut<PosedBlocks>,
) {
    *output = PosedBlocks::default();
    let Some((dimension, graphics)) = presentation.special_inputs().filter(|(d, _)| {
        controller.java.is_some() && controller.java_world.terrain.dimension() == Some(d)
    }) else {
        return;
    };
    if !Arc::ptr_eq(&graphics.runtime, &textures.assets) {
        return;
    }
    let terrain = &controller.java_world.terrain;
    let partial =
        (clock.elapsed.as_secs_f32() / simulation_clock::TICK_DURATION.as_secs_f32()).clamp(0., 1.);
    let mut solid = Vec::new();
    let mut blend = Vec::new();
    let result = (|| -> Result<(), String> {
        let mut parts = Vec::new();
        let mut active = std::collections::BTreeSet::new();
        for (&position, moving) in &terrain.pistons {
            if terrain.block_generation(position) != Some(moving.generation) {
                continue;
            }
            let state = graphics
                .registry
                .state(moving.state)
                .ok_or("missing Java moved state")?;
            let snapshot = world::java::piston::Snapshot {
                name: state.name.to_string(),
                properties: state.properties.clone(),
                facing: moving.facing,
                progress: 0.,
                extending: moving.extending,
                source: moving.source,
            };
            let plan = Plan::resolve(
                &graphics.registry,
                &snapshot,
                moving.motion.progress(partial),
            )
            .map_err(str::to_string)?;
            let offset = moving
                .motion
                .offset(moving.facing, moving.extending, partial)
                .ok_or("invalid Java piston offset")?;
            active.insert(position);
            let step =
                world::java::piston::step(moving.facing).ok_or("invalid piston direction")?;
            // Include both ends and the source base/head. Static mesh jobs can lag the event.
            active.insert(std::array::from_fn(|i| position[i] + step[i]));
            active.insert(std::array::from_fn(|i| position[i] - step[i]));
            for (id, sample_offset, translation) in [
                (plan.moving, plan.sampling_offset, offset),
                (plan.base, [0; 3], [0.; 3]),
            ] {
                if let Some(id) = id {
                    parts.push((position, id, sample_offset, translation));
                }
            }
        }
        clock.replaced.extend(&active);
        clock.replaced.retain(|p| {
            terrain.block(*p).is_some()
                && (active.contains(p) || !presentation.cell_resident(terrain, *p))
        });
        // Until the current mesh reaches the GPU, render the final authoritative cells here too.
        // This is a publication handoff, never a predicted server block or clock extension.
        for &position in &clock.replaced {
            if let Some(id) = terrain.block(position) {
                parts.push((position, id.0, [0; 3], [0.; 3]));
            }
        }
        for (position, id, sample_offset, translation) in parts {
            let biome_position: [i32; 3] = std::array::from_fn(|i| position[i] + sample_offset[i]);
            let Some(column) = terrain.column([
                biome_position[0].div_euclid(16),
                biome_position[2].div_euclid(16),
            ]) else {
                continue;
            };
            let sy = (biome_position[1] - dimension.min_y)
                .div_euclid(16)
                .clamp(0, column.sections.len() as i32 - 1) as usize;
            let biome = column.sections[sy]
                .biomes
                .get(
                    ((biome_position[1].rem_euclid(16) / 4) * 16
                        + (biome_position[2].rem_euclid(16) / 4) * 4
                        + biome_position[0].rem_euclid(16) / 4) as usize,
                )
                .ok_or("missing Java moving-block biome")?
                .0;
            if !graphics.contains(id) {
                continue;
            }
            let sample = std::array::from_fn(|i| position[i] + sample_offset[i]);
            for template in graphics.select(id, sample).map_err(|e| e.to_string())? {
                let authored = &graphics.templates[&template];
                let gpu = &graphics.runtime.model_templates()[template as usize];
                for (q, quad) in authored.quads.iter().enumerate() {
                    let geometry = &graphics.runtime.model_quads()[gpu.quad_start as usize + q];
                    let normal = assets::registry::java::geometry::normal(quad.facing).to_array();
                    let neighbor = std::array::from_fn(|i| sample[i] + normal[i] as i32);
                    let boundary = quad.positions.iter().all(|v| {
                        let dot = v.iter().zip(normal).map(|(p, n)| p * n).sum::<f32>();
                        (dot - if normal.iter().any(|n| *n > 0.) {
                            1.
                        } else {
                            0.
                        })
                        .abs()
                            < 0.00001
                    });
                    let light_position = if boundary { neighbor } else { sample };
                    let Some(light) = terrain
                        .column([
                            light_position[0].div_euclid(16),
                            light_position[2].div_euclid(16),
                        ])
                        .and_then(|c| c.light.sample(&dimension, light_position))
                    else {
                        continue;
                    };
                    let light =
                        u32::from(light[0].max(quad.light_emission)) | (u32::from(light[1]) << 4);
                    let material = graphics.runtime.material(geometry.material);
                    let list = if material.flags & assets::MATERIAL_FLAG_ALPHA_BLEND != 0 {
                        &mut blend
                    } else {
                        &mut solid
                    };
                    for corner in [0, 1, 2, 0, 2, 3] {
                        list.push(Vertex {
                            origin: sample,
                            position: std::array::from_fn(|i| {
                                geometry.positions[corner][i] as f32
                                    / assets::MODEL_POSITION_SCALE as f32
                                    - sample_offset[i] as f32
                                    + translation[i]
                            }),
                            uv: geometry.uvs[corner]
                                .map(|v| v as f32 / assets::MODEL_UV_SCALE as f32),
                            normal,
                            material: geometry.material,
                            light,
                            flags: geometry.flags,
                            biome,
                        });
                    }
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        presentation.special_failure(error);
        return;
    }
    *output = PosedBlocks {
        identity: Some(textures.binding_identity()),
        replaced: clock.replaced.iter().copied().collect::<Vec<_>>().into(),
        solid: solid.into(),
        blend: blend.into(),
    };
}
