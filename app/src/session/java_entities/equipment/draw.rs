//! Draws share the admitted body's identity, sampled pose and world lighting.
use super::*;
use client_presentation::presentation::actors::ActorPresentationBatch;
use render_model::ActorRigSubmission;
use render_model::{ActorGlint, HandItemAlphaMode, equipment::java_item};
pub(in crate::session::java_entities) fn append(
    batch: &mut ActorPresentationBatch,
    entry: &Entry,
    gear: &client_world::java::equipment::Equipment,
    body: &ActorRigSubmission,
    body_model: &render_model::actor::java_geometry::Model,
    posture: world::java::entities::animation::PlayerPosture,
    seconds: f32,
) -> Result<(), String> {
    for (index, draw) in entry.draws.iter().enumerate() {
        // Removal/different-item facts take effect before asynchronous replacement artwork.
        if !draw
            .source
            .as_ref()
            .is_some_and(|source| source.matches(gear))
        {
            continue;
        }
        let (bones, bounds) = if let Some(model) = &draw.model {
            let mut rotations = Vec::new();
            let mut offsets = Vec::new();
            for (i, name) in world::java::entities::animation::PLAYER_PARTS
                .iter()
                .enumerate()
            {
                let bone = model
                    .bone(name)
                    .ok_or("native armor posture bone missing")?;
                rotations.push((bone, posture.rotations[i]));
                offsets.push((bone, posture.offsets[i]));
            }
            let bones = model
                .pose_with_offsets_and_visibility(&rotations, &offsets, &[])
                .map_err(|e| e.to_string())?;
            let bounds = model.posed_bounds(&bones).map_err(|e| e.to_string())?;
            (bones, bounds)
        } else {
            let bone = match draw.attachment.ok_or("native item attachment missing")? {
                Attachment::Skull => {
                    let head = body_model
                        .bone("root/head")
                        .ok_or("native player head bone missing")?;
                    java_item::skull_bone(body.input.current_bones[head])
                }
                Attachment::Hand(left) => {
                    let hand = body_model
                        .bone(if left {
                            "root/left_arm"
                        } else {
                            "root/right_arm"
                        })
                        .ok_or("native player arm bone missing")?;
                    let root = body_model
                        .bone("root")
                        .ok_or("native player root missing")?;
                    java_item::held_bone(
                        body.input.current_bones[hand],
                        body.input.current_bones[root],
                        left,
                        entry.signature.slim,
                    )
                }
            }
            .ok_or("invalid native item attachment")?;
            let bounds =
                java_item::posed_bounds(draw.bounds, &bone).ok_or("invalid native held bounds")?;
            (Arc::from([bone]), bounds)
        };
        let mut draw_call = body.clone();
        draw_call.input.identity.layer = u8::try_from(index + 1).map_err(|e| e.to_string())?;
        draw_call.input.rig = draw.rig;
        draw_call.input.current_bones = bones.clone();
        draw_call.input.previous_bones = bones;
        draw_call.culling_bounds = bounds;
        draw_call.texture_layer = draw.artwork.layer;
        draw_call.material.state = Some(::assets::EntityRenderMaterialState {
            alpha_test: draw.alpha != HandItemAlphaMode::Opaque,
            cull: true,
            blend: draw.alpha == HandItemAlphaMode::Blend,
            depth_write: draw.alpha != HandItemAlphaMode::Blend,
            ..Default::default()
        });
        if draw.glint {
            draw_call.material.kind = ::assets::EntityRenderMaterial::Glint;
            draw_call.material.glint = if matches!(draw.attachment, Some(Attachment::Hand(_))) {
                ActorGlint::item(seconds, [1.; 2])
            } else {
                ActorGlint {
                    profile: render_api::CombatVisuals::Modern,
                    time_seconds: seconds,
                    ..Default::default()
                }
            };
        }
        batch.artwork.insert(draw_call.input.identity, draw.artwork);
        batch.submissions.push(draw_call);
    }
    Ok(())
}
