//! Host binding of admitted native player status to the existing UI owner.
use super::SessionController;
use bevy::prelude::*;
use client_shell::ui_resources::UiRuntimeResource;

pub(crate) fn update(
    mut controller: Option<ResMut<SessionController>>,
    ui: Option<ResMut<UiRuntimeResource>>,
    presentation: Option<Res<super::java_render::Presentation>>,
    mut heads: Option<ResMut<super::java_items::Heads>>,
    time: Res<Time<bevy::time::Real>>,
    mut chat: Local<super::java_chat::Projection>,
) {
    let Some(mut ui) = ui else {
        return;
    };
    if let Some(controller) = controller.as_mut() {
        super::java_inventory::flush(controller, &mut ui);
    }
    match controller
        .as_ref()
        .and_then(|c| c.java.as_ref().and(c.java_world.hud_snapshot()))
    {
        Some(snapshot) => {
            let Some(heads) = heads.as_deref_mut() else {
                if let Some(java) = controller
                    .as_mut()
                    .and_then(|controller| controller.java.as_mut())
                {
                    java.diagnostic_notice("native_item_head_resource_owner_unavailable");
                }
                return;
            };
            ui.publish_java_hud(snapshot);
            if let Some(names) = presentation
                .as_ref()
                .and_then(|p| p.item_names(snapshot.session, snapshot.epoch))
            {
                ui.publish_java_item_names(snapshot.session, snapshot.epoch, names);
            }
            if let Some(controller) = controller.as_ref() {
                chat.publish(
                    &controller.java_world,
                    &mut ui,
                    u64::try_from(time.elapsed().as_millis()).unwrap_or(u64::MAX),
                );
                ui.publish_java_sidebar(&controller.java_world);
                ui.publish_java_inventory(&controller.java_world);
                if let Some(items) = presentation
                    .as_ref()
                    .and_then(|p| p.items(snapshot.session, snapshot.epoch))
                {
                    ui.publish_java_item_catalog(
                        snapshot.session,
                        snapshot.epoch,
                        items,
                        &controller.java_world,
                        heads.update(
                            (snapshot.session, snapshot.epoch),
                            &controller.java_world,
                            time.elapsed(),
                        ),
                    );
                }
            }
            if ui.java_sidebar_has_unsupported_text()
                && let Some(java) = controller.as_mut().and_then(|c| c.java.as_mut())
            {
                java.diagnostic_notice("java_sidebar_component_not_implemented");
            }
        }
        None => {
            ui.clear_java_hud();
            if controller
                .as_ref()
                .is_none_or(|c| c.java.is_none() || c.java_world.terrain.dimension().is_none())
                && let Some(heads) = heads.as_deref_mut()
            {
                heads.clear();
            }
        }
    }
}
