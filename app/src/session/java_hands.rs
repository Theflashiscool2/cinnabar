//! Native item resources use the existing near-camera hand pass, with no gameplay/tick owner.
use super::{SessionController, java_movement, java_render};
use bevy::prelude::*;
use client_presentation::{
    actor_publication::{HAND_FOV_DEGREES, hand_motion_matrix, java_light_matrix},
    camera::{CameraSettingsAuthority, FirstPersonHandMotion},
};
use client_shell::{menu::MenuRuntime, ui_resources::UiRuntimeResource};
use render_model::{
    ActorMaterial, ActorRenderIdentity, ActorRigFrameBuilder, ActorRigRenderInput, ActorRigRoute,
    ActorRigSubmission, HandRigLight, RenderBoneTransform, equipment::java_item::Prepared,
};
use render_scene::HandRigScene;
mod use_pose;
use std::sync::Arc;
#[derive(Default)]
pub(crate) struct State {
    scope: Option<(u64, u64)>,
    items: [Option<Arc<Prepared>>; 2],
    builder: Option<ActorRigFrameBuilder>,
    serial: u64,
    bones: Option<Arc<[RenderBoneTransform]>>,
}
/// Runs after ordinary actor publication. Only the active Java edition owns this override.
#[allow(
    clippy::too_many_arguments,
    reason = "binds existing UI, camera, native authority and GPU ownership boundaries"
)]
pub(crate) fn update(
    controller: Option<Res<SessionController>>,
    terrain: Option<Res<java_render::Presentation>>,
    ui: Res<UiRuntimeResource>,
    menu: Res<MenuRuntime>,
    camera: Res<CameraSettingsAuthority>,
    server_camera: Option<Res<client_presentation::camera::ServerCameraView>>,
    movement: Res<java_movement::Movement>,
    interaction: Res<super::java_interaction::Interaction>,
    clock: Res<super::java_cooldowns::Clock>,
    motion: Option<Res<FirstPersonHandMotion>>,
    artwork: Option<Res<render_scene::ActorArtworkResource>>,
    time: Res<Time<bevy::time::Real>>,
    mut state: Local<State>,
    mut scene: ResMut<HandRigScene>,
) {
    let Some(controller) = controller.filter(|s| s.java.is_some()) else {
        *state = State::default();
        return; // Leave the newly published Bedrock frame untouched.
    };
    let world = &controller.java_world;
    let Some(hud) = ui.java_hud() else {
        scene.clear();
        return;
    };
    let scope = (hud.session, hud.epoch);
    if state.scope != Some(scope) {
        *state = State {
            scope: Some(scope),
            ..Default::default()
        };
    }
    let preferences = menu.settings_snapshot().0;
    use launcher::menu::settings_options::SettingId;
    let visibility = client_presentation::presentation::visibility::GameplayOverlayVisibility::new(
        preferences.value(SettingId::HideHud) != 0,
        preferences.value(SettingId::HideHand) != 0,
        hud.mode == Some(world::java::hud::GameMode::Spectator),
    );
    if terrain.as_ref().and_then(|t| t.world_ready()) != Some(true)
        || !client_presentation::camera::hand_first_person(
            camera.perspective(),
            server_camera.as_deref(),
        )
        || menu.is_visible()
        || !visibility.hand
        || ui.local_player_alive() == Some(false)
    {
        scene.clear();
        return;
    }
    let stacks = [
        world.inventory.held().and_then(Option::as_ref),
        world
            .inventory
            .slot(0, inventory::java::OFFHAND_MENU_SLOT)
            .and_then(Option::as_ref),
    ];
    let items = std::array::from_fn(|i| {
        stacks[i].and_then(|stack| {
            let key = assets::registry::java::items::key(
                &stack.item,
                stack.count,
                &stack.added,
                &stack.removed,
            );
            ui.java_held_item(&key, i == 1)
        })
    });
    if state.builder.is_none()
        || (0..2).any(|i| match (&state.items[i], &items[i]) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        })
    {
        match ActorRigFrameBuilder::new(
            items
                .iter()
                .flatten()
                .flat_map(|p| p.groups.iter().cloned()),
        ) {
            Ok(builder) => {
                state.builder = Some(builder);
                state.items = items;
            }
            Err(error) => {
                warn!(target:"java_session",?error,"Native held geometry rejected");
                scene.clear();
                return;
            }
        }
    }
    let Some(position) = world.player_motion.as_ref() else {
        scene.clear();
        return;
    };
    let eye = [
        position.position[0],
        position.position[1] + movement.0.eye_height(world),
        position.position[2],
    ];
    let Some([block, sky]) = world.terrain.light(eye.map(|v| v.floor() as i32)) else {
        scene.clear();
        return;
    };
    let look = super::java_look::view_rotation([position.yaw, position.pitch]);
    let light = HandRigLight {
        block_level: u32::from(block),
        sky_level: u32::from(sky),
        daylight: 1.,
        ..Default::default()
    }
    .with_java_lighting(java_light_matrix(motion.as_deref(), look));
    let motion = motion.as_deref().map_or(Mat4::IDENTITY, hand_motion_matrix);
    state.serial = state.serial.wrapping_add(1).max(1);
    let serial = state.serial;
    let bones = state
        .bones
        .get_or_insert_with(|| {
            Arc::from([RenderBoneTransform {
                rotation: [0., 0., 0., 1.],
                translation_scale: [0., 0., 0., 1.],
                axis_scale: [1.; 4],
            }])
        })
        .clone();
    let mut submissions = Vec::new();
    for hand in 0..2 {
        let Some(prepared) = &state.items[hand] else {
            continue;
        };
        // Verified FirstPersonHandsAndItemsRenderer.applyItemArmTransform at idle equip/swing.
        let use_pose = if interaction.0.using_hand()
            == Some(if hand == 0 {
                gameplay::java::item_use::plan::Hand::Main
            } else {
                gameplay::java::item_use::plan::Hand::Off
            }) {
            let consumable = stacks[hand].and_then(|s| {
                if s.removed.contains("minecraft:consumable") {
                    None
                } else {
                    s.added.get("minecraft:consumable").or_else(|| {
                        assets::registry::java::items::defaults::component(
                            &s.item,
                            "minecraft:consumable",
                        )
                    })
                }
            });
            let animation = consumable.and_then(|v| v.get("animation"));
            let eating = animation.is_none()
                || animation.is_some_and(|v| {
                    matches!(v.as_str(), Some("eat" | "drink")) || matches!(v.as_u64(), Some(1 | 2))
                });
            if consumable.is_some() && eating {
                use_pose::eat(
                    interaction.0.use_ticks().unwrap_or(0),
                    interaction.0.use_duration().unwrap_or(0),
                    0.,
                    hand == 0,
                )
            } else {
                Mat4::IDENTITY
            }
        } else {
            Mat4::IDENTITY
        };
        let attack = use_pose::attack(
            world.player_visual.swing.sample(clock.fraction()),
            hand == 1,
            interaction.0.using_hand().is_some_and(|using| {
                matches!(
                    (using, hand),
                    (gameplay::java::item_use::plan::Hand::Main, 0)
                        | (gameplay::java::item_use::plan::Hand::Off, 1)
                )
            }),
        );
        let transform = (motion
            * use_pose
            * Mat4::from_translation(Vec3::new(
                if hand == 0 { 0.56 } else { -0.56 },
                -0.52,
                -0.72,
            ))
            * attack)
            .transpose()
            .to_cols_array_2d();
        let shiny = stacks[hand]
            .is_some_and(|s| assets::registry::java::items::glint(&s.item, &s.added, &s.removed));
        for (layer, geometry) in prepared.groups.iter().enumerate() {
            let identity = ActorRenderIdentity {
                session_id: scope.0,
                dimension: 0,
                runtime_id: hand as u64 + 1,
                spawn_revision: std::num::NonZeroU64::new(scope.1.saturating_add(1)).unwrap(),
                ingress_sequence: world.inventory.revision.max(1),
                source_tick: None,
                movement_revision: world.inventory.revision,
                pose_generation: render_api::PoseGeneration::new(serial),
                layer: layer as u8,
            };
            let mut material = ActorMaterial::default();
            if shiny {
                material.kind = assets::EntityRenderMaterial::Glint;
                material.glint = render_model::ActorGlint::item(time.elapsed_secs(), [1.; 2]);
            }
            submissions.push(ActorRigSubmission {
                material,
                culling_bounds: prepared.bounds,
                input: ActorRigRenderInput {
                    identity,
                    rig: geometry.id,
                    previous_bones: bones.clone(),
                    current_bones: bones.clone(),
                    completed_tick: serial,
                    reset_generation: scope.1,
                },
                world_from_actor: [transform[0], transform[1], transform[2]],
                texture_layer: layer as u32
                    | render_model::HAND_ITEM_LAYER_FLAG
                    | prepared.alpha_modes[layer].texture_layer_flag()
                    | if hand == 1 {
                        render_model::HAND_OFFHAND_LAYER_FLAG
                    } else {
                        0
                    },
                route: ActorRigRoute::Compiled,
                tint: 0,
                overlay_rgba8: 0,
                uv_anim: [0., 0., 1., 1.],
                light: render_model::pack_actor_light(block, sky),
            });
        }
    }
    let frame = state.builder.as_mut().unwrap().build(0., None, submissions);
    // No body geometry is emitted; retain the existing admitted body-sampler binding.
    if scene.publish(
        frame,
        menu.player_skin().standard_skin().rgba8,
        light,
        HAND_FOV_DEGREES.to_radians(),
        serial,
    ) {
        scene.set_item_atlases(std::array::from_fn(|i| {
            state.items[i].as_ref().map(|p| p.atlas.clone())
        }));
        scene.set_item_glint(artwork.as_ref().and_then(|a| a.0.item_glint.clone()));
    }
}
