//! Server form validation and edition-preserving persistence.
use super::*;
impl MenuRuntime {
    /// The draft's `host:port`; a host typed with its own port keeps it.
    pub(super) fn draft_endpoint(&self) -> String {
        let host = self.address.as_str().trim();
        let port = self.port.as_str().trim();
        let own_port = host.contains("]:")
            || host.split_once(':').is_some_and(|(_, rest)| {
                !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
            });
        match (
            own_port || host.is_empty(),
            port.is_empty(),
            host.contains(':'),
        ) {
            (true, _, _) | (_, true, _) => host.to_owned(),
            (false, false, true) if host.starts_with('[') && host.ends_with(']') => {
                format!("{host}:{port}")
            }
            (false, false, true) => format!("[{host}]:{port}"),
            (false, false, false) => format!("{host}:{port}"),
        }
    }

    pub(super) fn save_draft(&mut self) -> bool {
        let name = self.name.as_str().trim();
        let endpoint = self.draft_endpoint();
        let address = endpoint.trim();
        if name.is_empty() || address.is_empty() {
            self.message = Some("Enter a server name and address.".to_owned());
            return false;
        }
        if self.server_edition == launcher::menu::ServerEdition::Java
            && let Err(error) = launcher_host::java::validate_address(address)
        {
            self.message = Some(format!("Invalid Java server address: {error}"));
            return false;
        }
        let server = SavedServer {
            edition: self.server_edition,
            name: name.to_owned(),
            address: address.to_owned(),
            favorite: false,
            last_joined_unix: 0,
        };
        let saved = if let Some(index) = self.editing.filter(|&index| index < self.servers.len()) {
            let existing = &mut self.servers[index];
            existing.name = server.name;
            existing.address = server.address;
            existing.edition = server.edition;
            index
        } else if let Some(index) = self.servers.iter().position(|existing| {
            existing.edition == server.edition
                && existing.address.eq_ignore_ascii_case(&server.address)
        }) {
            let existing = &mut self.servers[index];
            *existing = SavedServer {
                favorite: existing.favorite,
                last_joined_unix: existing.last_joined_unix,
                ..server
            };
            index
        } else {
            self.servers.push(server);
            self.servers.len() - 1
        };
        // The server just saved replaces whichever one was selected before.
        self.feeds.select_saved(saved);
        if let Some(external) = &mut self.external {
            external.preferences_dirty = true;
            return true;
        }
        if let Err(error) = self.saves.save(&self.servers) {
            self.message = Some(format!("Could not save server: {error}"));
            return false;
        }
        true
    }

    /// Queues the list for writing; a schema refusal leaves only the log.
    pub(super) fn save_servers(&mut self) {
        if let Some(external) = &mut self.external {
            external.preferences_dirty = true;
            return;
        }
        if let Err(error) = self.saves.save(&self.servers) {
            bevy::log::warn!("saved servers not written: {error:#}");
        }
    }

    /// Surfaces a saved-server write that failed on the worker.
    pub fn poll_saves(&mut self) {
        if let Some(error) = self.saves.take_error() {
            self.message = Some(format!("Could not save servers: {error}"));
        }
    }
}
