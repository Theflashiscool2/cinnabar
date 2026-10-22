//! Host binding of admitted native player status to the existing UI owner.
use super::SessionController;
use bevy::prelude::*;
use client_shell::ui_resources::UiRuntimeResource;

pub(crate) fn update(
    controller: Option<Res<SessionController>>,
    ui: Option<ResMut<UiRuntimeResource>>,
) {
    let Some(mut ui) = ui else {
        return;
    };
    match controller
        .as_ref()
        .and_then(|c| c.java.as_ref().and(c.java_world.hud_snapshot()))
    {
        Some(snapshot) => {
            ui.publish_java_hud(snapshot);
        }
        None => ui.clear_java_hud(),
    }
}
