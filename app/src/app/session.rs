use super::*;

/// Orders transfer, failure recovery and acceptance completion after frame publication.
pub(crate) fn configure_finish_system(app: &mut App) {
    app.init_resource::<crate::session::java_special::Presentation>()
        .add_systems(
            Update,
            crate::session::java_special::update
                .after(crate::session::java_render::update)
                .after(crate::block_entities::update_block_entity_scene),
        )
        .add_systems(
            Update,
            crate::session::java_special::advance
                .after(drive_session)
                .in_set(ClientFrameSet::NetworkSend),
        )
        .init_resource::<crate::session::java_entities::Presentation>()
        .init_resource::<crate::session::java_render::Presentation>()
        .init_resource::<crate::session::java_movement::Movement>()
        .init_resource::<crate::session::java_interaction::Interaction>()
        .add_systems(
            Update,
            crate::session::java_interaction::update
                .after(crate::session::java_special::advance)
                .after(crate::session::java_movement::update)
                .in_set(ClientFrameSet::NetworkSend),
        )
        .add_systems(
            Update,
            crate::session::java_movement::update.in_set(ClientFrameSet::NetworkSend),
        )
        .add_systems(
            Update,
            crate::session::java_render::update
                .after(drive_session)
                .after(drive_world_stream)
                .before(ChunkRenderApplySet)
                .in_set(ClientFrameSet::WorldPublication),
        );
    #[cfg(feature = "acceptance")]
    app.add_systems(
        Update,
        finish_acceptance_run
            .after(ClientFrameSet::NetworkSend)
            .after(ClientFrameSet::UiPublication)
            .after(record_metrics)
            .after(recover_session_failure),
    );
    app
        // The launcher gets first refusal on a fatal session error, so a failed
        // join returns to the menu instead of ending the process. This has to sit
        // after the failure is recorded (network drain) and before both systems
        // that act on it. The transfer follower runs first so a server-directed
        // move is classified as a replacement handoff, not a failure.
        .add_systems(
            Update,
            (follow_server_transfer, recover_session_failure)
                .chain()
                .run_if(
                    bevy::prelude::resource_exists::<
                        client_interaction::movement::PhysicsCollisionRegistries,
                    >,
                )
                .after(receive_network_events)
                .after(ClientFrameSet::NetworkSend)
                .after(ClientFrameSet::UiPublication)
                .before(exit_on_fatal_runtime_error),
        );
}
