//! Edition routing and background discovery, separate from Bedrock authentication.
use super::*;
impl MenuRuntime {
    pub(super) fn poll_java_discovery(&mut self) {
        let targets: Vec<_> = if self.visible && !self.is_connecting() {
            self.servers
                .iter()
                .filter(|s| s.edition == launcher::menu::ServerEdition::Java)
                .map(|s| s.address.clone())
                .collect()
        } else {
            Vec::new()
        };
        let now = std::time::Instant::now();
        if targets != self.java_targets
            || (now >= self.java_next_probe
                && self.java_discovery.as_ref().is_none_or(|d| d.finished()))
        {
            self.java_discovery = None;
            self.java_targets = targets.clone();
            if !targets.is_empty() {
                self.java_discovery = Some(launcher_host::java::Discovery::spawn(targets));
            }
            self.java_next_probe = now + std::time::Duration::from_secs(30);
        }
        if let Some(discovery) = &mut self.java_discovery {
            while let Some(result) = discovery.poll() {
                self.feeds
                    .pings
                    .insert(format!("java://{}", result.address), result.info);
            }
        }
    }

    /// Presents protocol progress without claiming a rendered/playable world.
    pub fn set_java_diagnostic(&mut self, message: String) {
        self.feeds.java_diagnostic = Some(message);
    }

    /// Queues an explicit edition; Java never starts the Bedrock account helper.
    pub fn request_edition_connect(
        &mut self,
        address: String,
        edition: launcher::menu::ServerEdition,
    ) {
        if edition == launcher::menu::ServerEdition::Bedrock {
            self.request_connect(address);
            return;
        }
        if !self
            .java_accounts
            .as_ref()
            .is_some_and(|manager| manager.ready_for_join())
        {
            self.java_pending_server = Some(address);
            self.java_account_action(launcher::java_accounts::Action::Open);
            return;
        }
        self.queue_java_join(address);
    }

    /// Explicit developer fixture entry; no stored account is read or replaced.
    #[cfg(feature = "developer-control")]
    pub fn request_java_fixture(&mut self, address: String) -> Result<(), String> {
        let socket = address
            .parse::<std::net::SocketAddr>()
            .map_err(|_| "Java fixture requires an explicit loopback IP:port".to_string())?;
        if !socket.ip().is_loopback() {
            return Err("Java fixture requires loopback".into());
        }
        self.queue_java_join(address);
        Ok(())
    }

    fn queue_java_join(&mut self, address: String) {
        let edition = launcher::menu::ServerEdition::Java;
        self.feeds.java_diagnostic = Some("Preparing Java authentication".to_owned());
        self.local_world_joined = false;
        self.remember_session_origin();
        self.stop_catalog();
        self.intents.join_sign_in_deadline = None;
        self.intents.join_sign_in_check = false;
        self.remember_retry_target(&address, None, false);
        if let Some(target) = self.retry_target.as_mut() {
            target.edition = edition;
        }
        self.intents.join = Some(JoinIntent {
            address,
            edition,
            auth_cache: None,
            local_world: false,
        });
        self.disconnect_message = None;
        self.show_connecting();
    }
}

impl MenuRuntime {
    pub(super) fn java_account_action(&mut self, action: launcher::java_accounts::Action) {
        use launcher::java_accounts::Action;
        if self.over_world() || self.is_connecting() {
            return;
        }
        if self.java_accounts.is_none() {
            let root = self
                .layout
                .auth_cache()
                .parent()
                .expect("auth directory")
                .join("java");
            self.java_accounts = Some(launcher_host::java_accounts::Manager::new(root));
        }
        let manager = self.java_accounts.as_mut().unwrap();
        match action {
            Action::Setup => {
                self.java_client_id.set_text(&manager.view.client_id_input);
                manager.request(Action::Cancel);
                manager.view.setup = true;
                self.field = None;
            }
            Action::EditClientId => self.focus_field(launcher::menu::MenuField::JavaClientId),
            Action::OpenRegistration => manager.open_registration(),
            Action::SaveSetup => {
                if manager.configure_input(self.java_client_id.as_str()) {
                    manager.view.setup = false;
                    self.field = None;
                }
            }
            Action::Open => {
                manager.view.open = true;
                manager.request(Action::Open);
                self.dialog = Some(launcher::menu::MenuDialog::Accounts);
                self.focused = 0;
            }
            Action::OpenLink => {
                manager.open_link();
            }
            Action::Cancel => {
                manager.request(Action::Cancel);
                manager.view.open = false;
                manager.view.setup = false;
                self.field = None;
                self.dialog = None;
                self.java_pending_server = None;
            }
            Action::Continue if manager.view.ready && !manager.view.busy => {
                self.field = None;
                manager.view.open = false;
                self.dialog = None;
                if let Some(address) = self.java_pending_server.take() {
                    self.request_edition_connect(address, launcher::menu::ServerEdition::Java);
                }
            }
            Action::Continue => {}
            action => manager.request(action),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher::menu::ServerEdition;
    #[test]
    fn java_route_never_waits_for_bedrock_sign_in_and_retry_retains_edition() {
        let mut menu = MenuRuntime::new(true, 2, "Fixture".into());
        menu.request_edition_connect("127.0.0.1".into(), ServerEdition::Java);
        assert!(menu.take_join_intent().is_none());
        assert_eq!(menu.java_pending_server.as_deref(), Some("127.0.0.1"));
        assert!(menu.java_accounts.as_ref().unwrap().view.open);
        // UI fixture only: no real authentication or account credential is fabricated.
        let manager = menu.java_accounts.as_mut().unwrap();
        manager.view.ready = true;
        manager.view.busy = false;
        manager.view.accounts.push(
            serde_json::from_value(serde_json::json!({
                "profile":{"id":"00000000-0000-4000-8000-000000000002","name":"Fixture"},
                "expires_at":u64::MAX
            }))
            .unwrap(),
        );
        manager.view.selected = Some(manager.view.accounts[0].profile.id);
        menu.java_account_action(launcher::java_accounts::Action::Continue);
        let intent = menu
            .take_join_intent()
            .expect("Java join without Bedrock account");
        assert_eq!(intent.edition, ServerEdition::Java);
        assert!(intent.auth_cache.is_none());
        assert_eq!(
            menu.retry_target.as_ref().unwrap().edition,
            ServerEdition::Java
        );
        assert!(menu.intents.join_sign_in_deadline.is_none());
        menu.disconnect_message = Some("Fixture disconnected".into());
        menu.feeds.account_active_id = Some("changed-bedrock-account".into());
        assert!(
            menu.can_reconnect(),
            "Java retry must not depend on Bedrock account selection"
        );
    }
    #[test]
    fn edition_default_preserves_srv_and_bracketed_ipv6() {
        let mut menu = MenuRuntime::new(true, 2, "Fixture".into());
        menu.activate(MenuAction::PlayAddServer);
        menu.activate(MenuAction::SelectServerEdition(ServerEdition::Java));
        menu.address.set_text("example.test");
        assert_eq!(menu.draft_endpoint(), "example.test");
        menu.address.set_text("[::1]");
        menu.port.set_text("25566");
        assert_eq!(menu.draft_endpoint(), "[::1]:25566");
    }
    #[test]
    fn editing_java_preserves_implicit_port_and_virtual_hostname() {
        let mut menu = MenuRuntime::new(true, 2, "Fixture".into());
        for address in ["example.test", "[::1]", "[::1]:25566"] {
            menu.servers = vec![SavedServer {
                edition: ServerEdition::Java,
                name: "Fixture".into(),
                address: address.into(),
                favorite: false,
                last_joined_unix: 0,
            }]
            .into();
            menu.activate(MenuAction::EditSaved(0));
            assert_eq!(menu.draft_endpoint(), address);
        }
    }
}
