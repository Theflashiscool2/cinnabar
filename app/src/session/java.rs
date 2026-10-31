//! Java joins share retirement/generation ownership but never start the Bedrock core.
use super::*;
pub(super) fn connect(
    menu: &mut MenuRuntime,
    session: &mut SessionResources<'_>,
    intent: JoinIntent,
) {
    let generation = session.retire();
    session.controller.java_world.begin(generation);
    menu.begin_join_progress(&intent.address, false);
    menu.show_connecting();
    session.controller.connecting = true;
    #[cfg(feature = "developer-control")]
    if let Ok(name) = std::env::var("CINNABAR_JAVA_OFFLINE_FIXTURE") {
        match intent.address.parse() {
            Ok(address) => {
                session.controller.java = Some(
                    client_session::java::Session::spawn(java_protocol::session::Options::fixture(
                        address, name, generation,
                    ))
                    .with_report_directory(menu.layout().log_dir()),
                )
            }
            Err(error) => session
                .controller
                .fail_join(menu, format!("Invalid Java fixture address: {error}")),
        }
        return;
    }
    let root = menu
        .layout()
        .auth_cache()
        .parent()
        .expect("authentication directory")
        .join("java");
    session.controller.java = Some(
        client_session::java::Session::run(move |events, commands| {
            launcher_host::java::run(intent.address, generation, root, events, commands)
        })
        .with_report_directory(menu.layout().log_dir()),
    );
}
pub(super) fn poll(menu: &mut MenuRuntime, session: &mut SessionResources<'_>) {
    let controller = &mut *session.controller;
    let Some(java) = controller.java.as_mut() else {
        return;
    };
    for _ in 0..64 {
        if let Some((revision, command)) = controller.java_world.correction_command() {
            match java.command(command) {
                Ok(()) => controller.java_world.correction_queued(revision),
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => break,
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    controller.java_world.terrain_error = Some("Java correction transport closed");
                    break;
                }
            }
        }
        let Some(event) = java.poll() else {
            break;
        };
        let display = !matches!(
            event.event,
            java_protocol::session::Event::DeferredPlay { .. }
                | java_protocol::session::Event::Terrain { .. }
        );
        let admitted = controller.java_world.admit(event);
        if !admitted {
            java.diagnostic_notice("world_event_not_admitted");
        }
        if admitted && display {
            let state = &controller.java_world;
            let message = if let Some(bootstrap) = &state.bootstrap {
                format!(
                    "Java {}: play protocol reached; world {}, dimension {}, {} registries, {} keepalives. Native terrain loading; gameplay is incomplete. Cancel to disconnect.",
                    java_protocol::target::GAME_VERSION,
                    bootstrap.world.spawn.world,
                    bootstrap.world.spawn.dimension_type,
                    bootstrap.configuration.registries.len(),
                    state.keepalives
                )
            } else {
                format!(
                    "Java {}: {:?}",
                    java_protocol::target::GAME_VERSION,
                    state.phase
                )
            };
            menu.set_java_diagnostic(message);
        }
    }
    if controller.java_world.deferred_entity_dynamics > 0 {
        java.diagnostic_notice("remote_entity_dynamics_deferred");
    }
    if controller.java_world.status.unimplemented_notifications > 0 {
        java.diagnostic_notice("hud_notifications_not_implemented");
    }
    if controller.java_world.status.odd_values > 0 {
        java.diagnostic_notice("invalid_hud_values_rejected");
    }
    if let Some(error) = controller.java_world.terrain_error {
        java.fail_admission(error);
        bevy::log::warn!(target: "java_session", error, "Java world admission failed");
        controller.java = None;
        controller.fail_join(menu, launcher::menu::disconnect::from_java(error));
        return;
    }
    if let Some(result) = java.result() {
        controller.java = None;
        controller.java_world.begin(controller.generation);
        let reason = result
            .err()
            .unwrap_or_else(|| "Java connection closed".to_owned());
        bevy::log::warn!(target: "java_session", %reason, "Java connection ended");
        controller.fail_join(
            menu,
            launcher::menu::disconnect::from_java(&format!(
                "{reason}\nClient: Java {} / protocol {}",
                java_protocol::target::GAME_VERSION,
                java_protocol::target::PROTOCOL
            )),
        );
    }
}
