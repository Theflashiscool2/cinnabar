//! Edition routing and core provisioning for a new connection.

use super::*;

/// Starts provisioning a fresh core and network session for one address,
/// replacing any previous session; [`poll_join`] finishes it on later frames.
///
/// This is the single replacement-handoff path shared by user joins and
/// automatic server-transfer follows: identity-checked session directory,
/// bounded core start wait, old-network shutdown, and a fresh session
/// generation for every attempt. A running launcher core takes the join
/// instead: it selects the target over `connect.v1` and the session dials it.
pub(super) fn attempt_connect(
    menu: &mut MenuRuntime,
    session: &mut SessionResources<'_>,
    cache: &BlobCache,
    intent: JoinIntent,
) {
    if intent.edition == launcher::menu::ServerEdition::Java {
        java::connect(menu, session, intent);
        return;
    }
    let profile = bridge::join_profile::JoinProfile::current();
    if profile.id().is_none() {
        bridge::join_profile::JoinProfile::begin();
    }
    bridge::join_profile::JoinProfile::current().mark("session_controller");
    let JoinIntent {
        edition: _,
        address,
        auth_cache,
        local_world,
    } = intent;
    menu.remember_retry_target(&address, auth_cache.as_deref(), local_world);
    // A replacement owns no route back into the old session, even when
    // provisioning the new endpoint fails before the connecting screen opens.
    menu.show_home();
    session.retire();
    session.controller.trust = None;
    session
        .state
        .runtime
        .session
        .experiences
        .select_destination(&address);
    menu.begin_join_progress(&address, local_world);
    let mut presence = presence_target(&address, local_world);
    if local_world {
        presence.join = menu.hosted_world_address();
        presence.max_players = menu.hosted_world_max_players();
    } else {
        presence.badge = menu.featured_badge(&address);
        presence.max_players = menu.destination_max_players(&address);
    }
    session.controller.presence = Some(presence);
    session.controller.local_world = local_world;
    session.controller.destination_identity = Some(
        if let Some(id) = menu.local_world_identity().filter(|_| local_world) {
            format!("local:{id}")
        } else {
            format!("remote:{}", address.trim().to_lowercase())
        },
    );
    session.controller.pending = Some(JoinIntent {
        edition: launcher::menu::ServerEdition::Bedrock,
        address,
        auth_cache,
        local_world,
    });
    session.controller.connecting = true;
    menu.show_connecting();
    prepare_join(menu, session, cache);
}

/// Starts dialing only after enabled grants and the launcher's cache mode agree.
fn provision_join(menu: &mut MenuRuntime, session: &mut SessionResources<'_>) {
    let Some(intent) = session.controller.pending.take() else {
        return;
    };
    let JoinIntent {
        edition: _,
        address,
        auth_cache,
        local_world,
    } = intent;
    let generation = session.controller.generation;
    let launcher = session.launcher.as_deref().and_then(|slot| {
        slot.begin_join(
            &address,
            local_world,
            auth_cache.is_some(),
            menu.is_launcher(),
        )
    });
    let controller = &mut *session.controller;
    if let Err(message) = controller.prepare_client_random_id(menu.layout()) {
        controller.fail_join(menu, message);
        return;
    }
    let stage = match launcher {
        Some(receiver) => JoinStage::Launcher(receiver),
        // A local world exists only behind the launcher core.
        None if local_world => {
            controller.fail_join(menu, format!("Could not open {address}: no launcher core"));
            return;
        }
        None => match controller.start_core(menu, &address, auth_cache.as_deref(), generation) {
            Ok(stage) => stage,
            Err(message) => {
                controller.fail_join(menu, message);
                return;
            }
        },
    };
    controller.join = Some(JoinAttempt {
        generation,
        address,
        auth_cache,
        local_world,
        stage,
    });
}

/// Captures the connection cache mode once enabled mods have finished admission.
pub(super) fn prepare_join(
    menu: &mut MenuRuntime,
    session: &mut SessionResources<'_>,
    cache: &BlobCache,
) {
    if session.controller.pending.is_none()
        || session
            .mod_registration
            .as_ref()
            .is_some_and(|watcher| watcher.preparing())
    {
        return;
    }
    let connection_cache = crate::modding::packets::PacketMods::connection_cache(
        cache,
        session.packet_mods.as_deref(),
        session
            .mod_registration
            .as_ref()
            .map(|watcher| watcher.active_grants())
            .unwrap_or_default(),
    );
    session.controller.blob_cache = connection_cache;
    provision_join(menu, session);
}
