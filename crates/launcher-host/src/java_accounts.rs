//! Separate Java account worker; no Bedrock account state or credentials enter here.
use java_protocol::auth;
use launcher::java_accounts::{self as store, Action, View};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

pub struct Manager {
    sender: tokio::sync::mpsc::Sender<(u64, Action, Option<auth::Profile>)>,
    published: Arc<Mutex<(u64, View)>>,
    generation: Arc<AtomicU64>,
    pub view: View,
}
impl std::fmt::Debug for Manager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JavaAccountManager")
    }
}
impl Manager {
    pub fn new(root: PathBuf) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(32);
        let published = Arc::new(Mutex::new((0, View::default())));
        let generation = Arc::new(AtomicU64::new(0));
        let state = published.clone();
        let current = generation.clone();
        let _ = std::thread::Builder::new()
            .name("java-accounts".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => runtime.block_on(run(root, receiver, state, current)),
                    Err(_) => {
                        state.lock().unwrap().1.message =
                            "Could not start Java account worker".into()
                    }
                }
            });
        Self {
            sender,
            published,
            generation,
            view: View::default(),
        }
    }
    pub fn request(&mut self, action: Action) {
        // Capture identity from the displayed list, not a later reordered disk snapshot.
        let target = match action {
            Action::Select(index) => self.view.accounts.get(index),
            _ => self
                .view
                .accounts
                .iter()
                .find(|account| Some(account.profile.id) == self.view.selected),
        }
        .map(|account| account.profile.clone());
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.view.busy = true;
        self.view.ready = false;
        self.view.prompt = None;
        self.view.message = "Working…".into();
        if self.sender.try_send((generation, action, target)).is_err() {
            self.view.busy = false;
            self.view.message = "Java account worker unavailable; retry".into();
        }
    }
    pub fn poll(&mut self) {
        if self.sender.is_closed() {
            self.view.busy = false;
            self.view.ready = false;
            self.view.message = "Java account worker stopped; restart the app".into();
            return;
        }
        let state = self.published.lock().unwrap();
        if state.0 == self.generation.load(Ordering::SeqCst) {
            let open = self.view.open;
            let setup = self.view.setup;
            self.view = state.1.clone();
            self.view.open = open;
            self.view.setup = setup;
        }
        if self.view.ready && !self.ready_for_join() {
            self.view.ready = false;
            self.view.message = "Expired — refresh the selected Java account".into();
        }
    }
    pub fn ready_for_join(&self) -> bool {
        self.view.ready
            && !self.view.busy
            && self.view.accounts.iter().any(|account| {
                Some(account.profile.id) == self.view.selected
                    && account.expires_at > auth::unix_now().saturating_add(60)
            })
    }
    /// Browser handoff does not cancel the polling operation or block the UI.
    pub fn open_link(&self) {
        let Some(prompt) = &self.view.prompt else {
            return;
        };
        self.open_url(prompt.verification_uri.clone());
    }
    pub fn open_registration(&self) {
        self.open_url("https://entra.microsoft.com/".into());
    }
    /// Validate public configuration locally; invalid input never starts authorization.
    pub fn configure_input(&mut self, input: &str) -> bool {
        match store::parse_application_id(input) {
            Some(id) => {
                self.request(Action::Configure(id));
                true
            }
            _ => {
                self.view.message = "Enter the Application (client) ID as a UUID, not a secret, password or directory ID.".into();
                *self.published.lock().unwrap() =
                    (self.generation.load(Ordering::SeqCst), self.view.clone());
                false
            }
        }
    }
    fn open_url(&self, url: String) {
        let state = self.published.clone();
        let current = self.generation.clone();
        let id = current.load(Ordering::SeqCst);
        let _ = std::thread::Builder::new().name("java-signin-browser".into()).spawn(move || {
            if !crate::desktop::open_sign_in_link(&url) && current.load(Ordering::SeqCst) == id {
                let mut state = state.lock().unwrap();
                if state.0 == id {
                    state.1.message = format!("Could not open Microsoft. Open {url} in your browser, then return here.");
                }
            }
        });
    }
}
fn snapshot(root: &std::path::Path) -> View {
    let mut view = View::default();
    match store::list(root) {
        Ok(accounts) => view.accounts = accounts,
        Err(error) => {
            view.message = error.to_string();
            return view;
        }
    }
    view.selected = store::active_id(root).ok();
    let client = configuration(root);
    view.configured = client.is_ok();
    view.client_id_input = client.as_ref().cloned().unwrap_or_default();
    view.labels = view
        .accounts
        .iter()
        .map(|summary| {
            let status = match store::load_id(root, summary.profile.id) {
                Ok(account)
                    if client
                        .as_ref()
                        .is_ok_and(|id| account.belongs_to_application(id)) =>
                {
                    if account.valid_at(auth::unix_now()) {
                        "ready"
                    } else {
                        "expired"
                    }
                }
                Ok(_) => "needs sign-in",
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => "needs sign-in",
                Err(_) => "failed",
            };
            format!("{} — {status}", summary.profile.name)
        })
        .collect();
    match store::active(root) {
        Ok(account) if account.valid_at(auth::unix_now()) => match configuration(root) {
            Ok(client_id) if account.belongs_to_application(&client_id) => {
                view.ready = true;
                view.message = "Ready".into();
            }
            Ok(_) => view.message = "Needs sign-in with the configured Java application".into(),
            Err(error) => view.message = error,
        },
        Ok(_) => {
            view.message = "Expired — refresh or add this account again to reauthenticate".into()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            view.message = "Needs Java sign-in".into()
        }
        Err(error) => view.message = error.to_string(),
    }
    view
}
fn configuration(root: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(root.join("oauth-client.json"))
        .map_err(|_| auth::Error::MissingClient.to_string())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Invalid Java OAuth configuration".to_owned())?;
    let id = value["client_id"]
        .as_str()
        .and_then(store::parse_application_id)
        .ok_or_else(|| auth::Error::MissingClient.to_string())?;
    Ok(id.to_string())
}
async fn run(
    root: PathBuf,
    mut commands: tokio::sync::mpsc::Receiver<(u64, Action, Option<auth::Profile>)>,
    published: Arc<Mutex<(u64, View)>>,
    generation: Arc<AtomicU64>,
) {
    let mut job: Option<(
        u64,
        tokio::task::JoinHandle<Result<auth::Account, String>>,
        bool,
    )> = None;
    loop {
        tokio::select! {
            biased;
            command = commands.recv() => {
                let Some((id, mut action, mut target)) = command else { break; };
                if let Some((_, task, _)) = job.take() { task.abort(); }
                if generation.load(Ordering::SeqCst) != id { continue; }
                if let Action::Configure(client_id) = action {
                    if let Err(error) = store::configure(&root, client_id) {
                        let mut view = snapshot(&root);
                        view.message = format!("Could not save Java setup: {error}");
                        *published.lock().unwrap() = (id, view);
                        continue;
                    }
                    action = Action::Add;
                }
                let mut view = snapshot(&root);
                if action == Action::Open && let Ok(account) = store::active(&root)
                    && !account.valid_at(auth::unix_now()) {
                    action = Action::Refresh;
                    target = Some(account.profile.clone());
                }
                let result: Result<(), String> = match action {
                    Action::Add | Action::Refresh => {
                        match configuration(&root).and_then(|id| auth::Client::new(id).map_err(|e| e.to_string())) {
                            Err(error) => Err(error),
                            Ok(client) => {
                                let old = if action == Action::Refresh {
                                    Some(target.as_ref().ok_or("No Java account selected".to_owned())
                                        .and_then(|profile| store::load_id(&root, profile.id).map_err(|e| e.to_string())))
                                } else { None };
                                view.busy = true; view.ready = false;
                                view.message = "Signing in to Minecraft Java…".into();
                                let prompt_state = published.clone();
                                let current = generation.clone();
                                let initial = view.clone();
                                let task = tokio::spawn(async move {
                                    match old {
                                        Some(account) => client.refresh(&account?, &Default::default()).await.map_err(|e| e.to_string()),
                                        None => client.sign_in(&Default::default(), move |prompt| {
                                            if current.load(Ordering::SeqCst) == id {
                                                let mut view = initial;
                                                view.message = "Open Microsoft, enter the code, and choose the account you want to add. Use another account on Microsoft's page if needed. Return here when finished.".into();
                                                view.prompt = Some(prompt);
                                                *prompt_state.lock().unwrap() = (id, view);
                                            }
                                        }).await.map_err(|e| e.to_string()),
                                    }
                                });
                                job = Some((id, task, action == Action::Refresh));
                                Ok(())
                            }
                        }
                    }
                    Action::Select(_) => target.ok_or("Java account no longer exists".to_owned())
                        .and_then(|account| store::select(&root, account.id).map_err(|e| e.to_string())),
                    Action::Remove => target.ok_or("No Java account selected".to_owned())
                        .and_then(|account| store::logout(&root, account.id).map_err(|e| e.to_string())),
                    _ => Ok(()),
                };
                if !view.busy { view = snapshot(&root); }
                if let Err(error) = result { view.busy = false; view.ready = false; view.message = error; }
                *published.lock().unwrap() = (id, view);
            }
            result = async { match job.as_mut() {
                Some((id, task, refresh)) => (*id, task.await, *refresh),
                None => std::future::pending().await,
            }} => {
                job = None;
                let (id, result, refresh) = result;
                if generation.load(Ordering::SeqCst) != id { continue; }
                let result = result.map_err(|_| "Java sign-in task failed".to_owned())
                    .and_then(|account| account).and_then(|account| {
                        if refresh { store::save_refreshed(&root, &account) }
                        else { store::save(&root, &account) }.map_err(|e| e.to_string())
                    });
                let mut view = snapshot(&root);
                if let Err(error) = result { view.ready = false; view.message = error; }
                *published.lock().unwrap() = (id, view);
            }
        }
    }
    if let Some((_, task, _)) = job {
        task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_application_setup_persists_without_account_credentials() {
        let root = tempfile::tempdir().unwrap();
        let public_id = "00000000-0000-4000-8000-000000000001";
        assert!(configuration(root.path()).is_err());
        store::configure(root.path(), store::parse_application_id(public_id).unwrap()).unwrap();
        assert_eq!(configuration(root.path()).unwrap(), public_id);
        let mut manager = Manager::new(root.path().into());
        manager.request(Action::Open);
        settle(&mut manager);
        assert!(manager.view.configured);
        assert!(!manager.view.ready);
        assert!(manager.view.accounts.is_empty());
        for invalid in [
            "",
            "not-an-application-id",
            "00000000-0000-0000-0000-000000000000",
        ] {
            assert!(!manager.configure_input(invalid));
            manager.poll();
            assert!(manager.view.message.contains("Application (client) ID"));
            assert!(!manager.view.busy);
            assert!(manager.view.prompt.is_none());
            assert_eq!(configuration(root.path()).unwrap(), public_id);
        }
        assert!(!root.path().join("active").exists());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[test]
    #[cfg(not(target_os = "ios"))]
    fn stored_fixture_accounts_survive_restart_select_and_remove_independently() {
        let root = tempfile::tempdir().unwrap();
        let client_id = "00000000-0000-4000-8000-000000000001";
        std::fs::write(
            root.path().join("oauth-client.json"),
            serde_json::to_vec(&serde_json::json!({"client_id":client_id})).unwrap(),
        )
        .unwrap();
        for (id, name) in [
            ("00000000-0000-4000-8000-000000000002", "Alpha"),
            ("00000000-0000-4000-8000-000000000003", "Beta"),
        ] {
            let account = serde_json::from_value(serde_json::json!({
                "profile":{"id":id,"name":name},"expires_at":u64::MAX,
                "client_id":client_id,"access_token":"fixture-only","refresh_token":"fixture-only"
            }))
            .unwrap();
            store::save(root.path(), &account).unwrap();
        }
        let mut manager = Manager::new(root.path().into());
        manager.request(Action::Open);
        settle(&mut manager);
        assert_eq!(manager.view.accounts.len(), 2);
        assert!(manager.view.ready);
        manager.request(Action::Select(0));
        settle(&mut manager);
        assert_eq!(
            manager.view.selected,
            Some(manager.view.accounts[0].profile.id)
        );
        drop(manager);
        let mut manager = Manager::new(root.path().into());
        manager.request(Action::Open);
        settle(&mut manager);
        assert_eq!(
            manager.view.selected,
            Some(manager.view.accounts[0].profile.id)
        );
        let kept = manager.view.accounts[1].profile.id;
        manager.request(Action::Remove);
        settle(&mut manager);
        assert_eq!(manager.view.accounts.len(), 1);
        assert_eq!(manager.view.accounts[0].profile.id, kept);
        assert!(manager.view.selected.is_none() && !manager.view.ready);
        assert!(store::load_id(root.path(), kept).is_ok());
    }
    fn settle(manager: &mut Manager) {
        test_time::eventually("Java account worker result", || {
            manager.poll();
            !manager.view.busy
        });
    }
    #[test]
    fn missing_configuration_never_starts_authorization_and_can_cancel_retry() {
        let root = tempfile::tempdir().unwrap();
        let mut manager = Manager::new(root.path().into());
        manager.request(Action::Open);
        settle(&mut manager);
        assert!(manager.view.accounts.is_empty() && !manager.view.ready);
        manager.request(Action::Add);
        settle(&mut manager);
        assert!(manager.view.message.contains("registered Microsoft"));
        assert!(manager.view.prompt.is_none());
        manager.request(Action::Cancel);
        settle(&mut manager);
        assert!(!manager.view.busy && !manager.view.ready);
        manager.request(Action::Add);
        settle(&mut manager);
        assert!(manager.view.message.contains("registered Microsoft"));
        assert!(!root.path().join("active").exists());
    }
    #[test]
    fn stale_worker_publication_cannot_replace_new_operation() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
        let published = Arc::new(Mutex::new((0, View::default())));
        let mut manager = Manager {
            sender,
            published: published.clone(),
            generation: Arc::new(AtomicU64::new(0)),
            view: View::default(),
        };
        manager.request(Action::Add);
        let first = receiver.try_recv().unwrap().0;
        manager.request(Action::Cancel);
        let second = receiver.try_recv().unwrap().0;
        let stale = View {
            ready: true,
            message: "stale login".into(),
            ..Default::default()
        };
        *published.lock().unwrap() = (first, stale);
        manager.poll();
        assert!(manager.view.busy && !manager.view.ready);
        *published.lock().unwrap() = (
            second,
            View {
                message: "Cancelled".into(),
                ..Default::default()
            },
        );
        manager.poll();
        assert!(!manager.view.busy && !manager.view.ready);
        assert_eq!(manager.view.message, "Cancelled");
    }
}
