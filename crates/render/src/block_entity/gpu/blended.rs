//! Transparent model, overlay and effect queueing; Java skin surfaces retain depth writes.
use super::*;
pub(super) fn queue_translucent(
    pipeline_cache: Res<PipelineCache>,
    mut pipeline: ResMut<BlockEntityPipeline>,
    gpu: Res<BlockEntityGpu>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    views: Query<BlockEntityView>,
) {
    let draw_function = draw_functions.read().id::<DrawTranslucentCommands>();
    queue_blended(
        &gpu.translucent,
        PipelineMode::Translucent,
        draw_function,
        &pipeline_cache,
        &mut pipeline,
        &mut phases,
        &views,
        |_, _| false,
    );
}

pub(super) fn queue_overlay(
    pipeline_cache: Res<PipelineCache>,
    mut pipeline: ResMut<BlockEntityPipeline>,
    gpu: Res<BlockEntityGpu>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    views: Query<BlockEntityView>,
) {
    let draw_function = draw_functions.read().id::<DrawOverlayCommands>();
    queue_blended(
        &gpu.overlay,
        PipelineMode::Overlay,
        draw_function,
        &pipeline_cache,
        &mut pipeline,
        &mut phases,
        &views,
        |_, _| false,
    );
}

#[allow(
    clippy::too_many_arguments,
    reason = "a queue system's render-world inputs"
)]
pub(super) fn queue_outline(
    pipeline_cache: Res<PipelineCache>,
    mut pipeline: ResMut<BlockEntityPipeline>,
    gpu: Res<BlockEntityGpu>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    views: Query<BlockEntityView>,
    pack: render_scene::shaderpack::ShaderPackDrawnEffects,
) {
    let draw_function = draw_functions.read().id::<DrawOutlineCommands>();
    queue_blended(
        &gpu.outline,
        PipelineMode::Outline,
        draw_function,
        &pipeline_cache,
        &mut pipeline,
        &mut phases,
        &views,
        |view, _| pack.replaces(view, |drawn| drawn.selection),
    );
}

#[allow(
    clippy::too_many_arguments,
    reason = "a queue system's render-world inputs"
)]
pub(super) fn queue_crack(
    pipeline_cache: Res<PipelineCache>,
    mut pipeline: ResMut<BlockEntityPipeline>,
    gpu: Res<BlockEntityGpu>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    views: Query<BlockEntityView>,
    pack: render_scene::shaderpack::ShaderPackDrawnEffects,
) {
    // Check partition ranges when drawing, after this frame's vertex upload.
    for highlight in [false, true] {
        let functions = draw_functions.read();
        let draw_function = if highlight {
            functions.id::<DrawHighlightCommands>()
        } else {
            functions.id::<DrawCrackCommands>()
        };
        queue_blended(
            &gpu.crack,
            PipelineMode::Crack,
            draw_function,
            &pipeline_cache,
            &mut pipeline,
            &mut phases,
            &views,
            |view, _| {
                pack.replaces(view, |drawn| {
                    if highlight {
                        drawn.selection
                    } else {
                        drawn.cracks
                    }
                })
            },
        );
    }
}

pub(super) fn queue_additive(
    pipeline_cache: Res<PipelineCache>,
    mut pipeline: ResMut<BlockEntityPipeline>,
    gpu: Res<BlockEntityGpu>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    views: Query<BlockEntityView>,
) {
    let draw_function = draw_functions.read().id::<DrawAdditiveCommands>();
    queue_blended(
        &gpu.additive,
        PipelineMode::Additive,
        draw_function,
        &pipeline_cache,
        &mut pipeline,
        &mut phases,
        &views,
        |_, _| false,
    );
}

/// Queues `list` on every camera `skip` does not reject, given the view and whether it runs
/// a shader pack (views whose pack draws the list itself).
#[allow(
    clippy::too_many_arguments,
    reason = "one blended list's queue inputs and its view filter"
)]
fn queue_blended(
    list: &VertexList,
    mode: PipelineMode,
    draw_function: DrawFunctionId,
    pipeline_cache: &PipelineCache,
    pipeline: &mut BlockEntityPipeline,
    phases: &mut ViewSortedRenderPhases<Transparent3d>,
    views: &Query<BlockEntityView>,
    skip: impl Fn(Entity, bool) -> bool,
) {
    if list.count == 0 || list.bind_group.is_none() {
        return;
    }
    for (view_entity, main_entity, view, extracted_camera, msaa, pack_view) in views {
        if skip(view_entity, pack_view) {
            continue;
        }
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let Ok(pipeline_id) = pipeline.variants.specialize(
            pipeline_cache,
            BlockEntityPipelineKey {
                mode,
                msaa: *msaa,
                hdr: extracted_camera.hdr,
            },
        ) else {
            continue;
        };
        if mode == PipelineMode::Translucent {
            let matrix = view.clip_from_world.unwrap_or_else(|| {
                view.clip_from_view * view.world_from_view.to_matrix().inverse()
            });
            let rangefinder = view.rangefinder3d();
            for run in &list.runs {
                if render_model::block_entity::draw_ranges::visible_ranges(
                    std::slice::from_ref(run),
                    matrix,
                )
                .next()
                .is_none()
                {
                    continue;
                }
                let center = (bevy::math::Vec3::from_array(run.low)
                    + bevy::math::Vec3::from_array(run.high))
                    * 0.5;
                crate::transparent_phase::add(
                    phase,
                    Transparent3d {
                        sorting_info:
                            bevy::core_pipeline::core_3d::TransparentSortingInfo3d::AlwaysOnTop,
                        entity: (view_entity, *main_entity),
                        pipeline: pipeline_id,
                        draw_function,
                        distance: rangefinder.distance(&center),
                        batch_range: 0..1,
                        extra_index: PhaseItemExtraIndex::IndirectParametersIndex {
                            range: run.vertices.clone(),
                            batch_set_index: None,
                        },
                        indexed: false,
                    },
                );
            }
            continue;
        }
        crate::transparent_phase::add(
            phase,
            Transparent3d {
                sorting_info: bevy::core_pipeline::core_3d::TransparentSortingInfo3d::AlwaysOnTop,
                entity: (view_entity, *main_entity),
                pipeline: pipeline_id,
                draw_function,
                // Existing overlays retain their authored final phase order.
                distance: 0.0,
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: false,
            },
        );
    }
}
