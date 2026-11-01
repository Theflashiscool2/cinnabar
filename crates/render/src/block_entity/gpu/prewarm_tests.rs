use super::*;
use crate::pipeline_warmup::{PrewarmPipelines, WarmView};

#[test]
fn first_block_selection_reuses_a_prewarmed_pipeline() {
    let (app, _) = crate::queue_review_support::app();
    let cache = app.world().resource::<PipelineCache>();
    let mut pipeline = BlockEntityPipeline::from_world(&mut World::new());
    for msaa in [Msaa::Off, Msaa::Sample2, Msaa::Sample4, Msaa::Sample8] {
        for hdr in [false, true] {
            let mut ids = Vec::new();
            let view = WarmView {
                msaa,
                hdr,
                enhanced: false,
                shader_pack: false,
                output: None,
            };
            pipeline.prewarm(cache, view, &mut ids).unwrap();
            let key = BlockEntityPipelineKey {
                mode: PipelineMode::Outline,
                msaa,
                hdr,
            };
            let outline = pipeline.variants.specialize(cache, key).unwrap();
            assert!(
                ids.contains(&outline),
                "selection outline must not compile on first target"
            );
        }
    }
}

#[test]
fn block_entity_coverage_is_prewarmed_only_for_solid_cutouts() {
    let (mut app, _) = crate::queue_review_support::app();
    let mut cache = app.world_mut().remove_resource::<PipelineCache>().unwrap();
    let mut pipeline = BlockEntityPipeline::from_world(&mut World::new());
    for msaa in [Msaa::Off, Msaa::Sample2, Msaa::Sample4, Msaa::Sample8] {
        let mut ids = Vec::new();
        pipeline
            .prewarm(
                &cache,
                WarmView {
                    msaa,
                    hdr: false,
                    enhanced: false,
                    shader_pack: false,
                    output: None,
                },
                &mut ids,
            )
            .unwrap();
        for mode in [
            PipelineMode::Solid,
            PipelineMode::Overlay,
            PipelineMode::Translucent,
            PipelineMode::Outline,
            PipelineMode::Crack,
            PipelineMode::Portal,
            PipelineMode::Additive,
        ] {
            let id = pipeline
                .variants
                .specialize(
                    &cache,
                    BlockEntityPipelineKey {
                        mode,
                        msaa,
                        hdr: false,
                    },
                )
                .unwrap();
            assert!(ids.contains(&id));
            let descriptor = crate::queue_review_support::queued_descriptor(&mut cache, id);
            if mode == PipelineMode::Translucent {
                assert!(
                    descriptor
                        .depth_stencil
                        .as_ref()
                        .unwrap()
                        .depth_write_enabled
                        .unwrap()
                );
                assert_eq!(descriptor.primitive.cull_mode, None);
                assert_eq!(
                    descriptor.fragment.as_ref().unwrap().targets[0]
                        .as_ref()
                        .unwrap()
                        .blend,
                    Some(BlendState::ALPHA_BLENDING)
                );
            }
            assert_eq!(
                descriptor.multisample.alpha_to_coverage_enabled,
                msaa.samples() > 1 && mode == PipelineMode::Solid
            );
        }
    }
}
