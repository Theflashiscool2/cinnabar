//! Java launcher account preparation; all file and service work runs off the rendering thread.
use java_protocol::{
    address::Address,
    auth,
    session::{Identity, Options},
};
use std::{path::PathBuf, time::Duration};

/// Uses the fork's public OAuth application configuration, never an upstream borrowed client ID.
pub async fn prepare(address: String, generation: u64, root: PathBuf) -> Result<Options, String> {
    let address: Address = address
        .parse()
        .map_err(|e: java_protocol::ProtocolError| e.to_string())?;
    let load_root = root.clone();
    let (client_id, account) = tokio::task::spawn_blocking(move || {
        let account = launcher::java_accounts::active(&load_root).map_err(|_| {
            "A separate Minecraft Java sign-in is required. The Bedrock/Xbox account does not sign you into Java servers. Java sign-in is not yet available in this app's account screen.".to_owned()
        })?;
        let config: serde_json::Value = serde_json::from_slice(
            &std::fs::read(load_root.join("oauth-client.json"))
                .map_err(|_| auth::Error::MissingClient.to_string())?,
        )
        .map_err(|_| "Invalid Java OAuth application configuration".to_owned())?;
        let client = config["client_id"]
            .as_str()
            .ok_or_else(|| auth::Error::MissingClient.to_string())?
            .to_owned();
        Ok::<_, String>((client, account))
    })
    .await
    .map_err(|_| "Java account task failed".to_owned())??;
    let _ = auth::Client::new(client_id.clone()).map_err(|e| e.to_string())?;
    if !account.belongs_to_application(&client_id) {
        return Err(
            "Java account belongs to a different OAuth application; sign in again".to_owned(),
        );
    }
    let report =
        java_protocol::transport::discover(&address, Duration::from_secs(5), &Default::default())
            .await
            .map_err(|e| e.to_string())?;
    if !report
        .status
        .version
        .as_ref()
        .is_some_and(|v| v.protocol == java_protocol::target::PROTOCOL)
    {
        return Err(format!(
            "Unsupported Java server version; select a Java {} server (protocol {})",
            java_protocol::target::GAME_VERSION,
            java_protocol::target::PROTOCOL
        ));
    }
    let account = if account.valid_at(auth::unix_now()) {
        account
    } else {
        let refreshed = auth::Client::new(client_id)
            .map_err(|e| e.to_string())?
            .refresh(&account, &Default::default())
            .await
            .map_err(|e| e.to_string())?;
        launcher::java_accounts::save_refreshed(&root, &refreshed)
            .map_err(|_| "Could not save refreshed Java credentials".to_owned())?;
        refreshed
    };
    Ok(Options {
        address,
        identity: Identity::Online(account),
        generation,
        connect_timeout: Duration::from_secs(10),
        login_timeout: Duration::from_secs(45),
        commands: None,
    })
}

#[derive(Clone, Debug)]
pub struct DiscoveryResult {
    pub address: String,
    pub info: launcher::menu::PingInfo,
}
/// One cancellable background round; dropping closes all probes. Address changes replace the round.
#[derive(Debug)]
pub struct Discovery {
    task: tokio::task::JoinHandle<()>,
    receiver: tokio::sync::mpsc::Receiver<DiscoveryResult>,
}
impl Discovery {
    pub fn spawn(addresses: Vec<String>) -> Self {
        static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .thread_name("java-discovery")
                .build()
                .expect("Java discovery executor")
        });
        let (tx, receiver) = tokio::sync::mpsc::channel(32);
        let task = runtime.spawn(async move {
            for address in addresses {
                let result = match address.parse() {
                    Ok(parsed) => {
                        java_protocol::transport::discover(
                            &parsed,
                            Duration::from_secs(5),
                            &Default::default(),
                        )
                        .await
                    }
                    Err(e) => Err(java_protocol::transport::QueryError::Protocol(e)),
                };
                let info = match result {
                    Ok(report) => {
                        let version = report.status.version.as_ref();
                        let compatible =
                            version.is_some_and(|v| v.protocol == java_protocol::target::PROTOCOL);
                        let version_text = version
                            .map(|v| v.name.as_str())
                            .unwrap_or("unknown version");
                        let motd = report
                            .status
                            .description
                            .as_ref()
                            .map(java_protocol::status::plain_text)
                            .unwrap_or_default();
                        let population = report.status.players.as_ref();
                        launcher::menu::PingInfo {
                            motd: format!(
                                "Java {version_text}: {motd}{}",
                                if compatible {
                                    " — protocol foundation; gameplay unavailable"
                                } else {
                                    " — incompatible"
                                }
                            ),
                            online: true,
                            players: population.map(|p| p.online.max(0) as u32).unwrap_or(0),
                            max_players: population.map(|p| p.max.max(0) as u32).unwrap_or(0),
                            ping_ms: report
                                .latency
                                .map(|v| v.as_millis().min(u128::from(u32::MAX - 1)) as u32)
                                .unwrap_or(u32::MAX),
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Java server discovery failed");
                        launcher::menu::PingInfo {
                            motd: discovery_message(&error).into(),
                            ..Default::default()
                        }
                    }
                };
                if tx.send(DiscoveryResult { address, info }).await.is_err() {
                    return;
                }
            }
        });
        Self { task, receiver }
    }
    pub fn poll(&mut self) -> Option<DiscoveryResult> {
        self.receiver.try_recv().ok()
    }
    pub fn finished(&self) -> bool {
        self.task.is_finished()
    }
}

/// Keep the actionable summary visible on a phone; full errors remain in the client log.
fn discovery_message(error: &java_protocol::transport::QueryError) -> &'static str {
    use java_protocol::transport::QueryError;
    use std::io::ErrorKind;
    match error {
        QueryError::Timeout => "Java timed out. Check address and network.",
        QueryError::Io(error) if error.kind() == ErrorKind::ConnectionRefused => {
            "Java refused connection. Check server port."
        }
        QueryError::Io(_) => "Java network error. Try explicit host:port; see logs.",
        QueryError::Protocol(_) => "Invalid Java status response. See client logs.",
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Validates a Java draft with the same address parser used by connection and discovery.
pub fn validate_address(input: &str) -> Result<(), String> {
    input
        .parse::<Address>()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Ends an online session if another process logs out or switches the selected Java account.
/// File reads stay on blocking workers, never on the frame/rendering thread.
pub async fn run(
    address: String,
    generation: u64,
    root: PathBuf,
    events: tokio::sync::mpsc::Sender<java_protocol::session::Envelope>,
    commands: tokio::sync::mpsc::Receiver<java_protocol::session::Command>,
) -> Result<(), String> {
    let mut options = prepare(address, generation, root.clone()).await?;
    options.commands = Some(commands);
    let id = match &options.identity {
        Identity::Online(account) => account.profile.id,
        _ => unreachable!("online preparation"),
    };
    let monitor = async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let root = root.clone();
            let selected =
                tokio::task::spawn_blocking(move || launcher::java_accounts::active_id(&root))
                    .await;
            if !matches!(selected,Ok(Ok(selected)) if selected==id) {
                return Err("Java account was switched or logged out; connection closed".to_owned());
            }
        }
    };
    tokio::select! { result=java_protocol::session::run(options,events,Default::default())=>result.map_err(|e|e.to_string()), result=monitor=>result }
}

#[cfg(test)]
mod account_preflight_tests {
    #[tokio::test]
    async fn signed_out_join_explains_java_account_before_discovery_or_app_config() {
        let root = tempfile::tempdir().unwrap();
        let result = super::prepare("unresolvable.invalid".into(), 1, root.path().into()).await;
        let error = match result {
            Ok(_) => panic!("signed-out join must not prepare a connection"),
            Err(error) => error,
        };
        assert!(error.contains("separate Minecraft Java sign-in"), "{error}");
        assert!(error.contains("Bedrock/Xbox"), "{error}");
        assert!(!root.path().join("oauth-client.json").exists());
    }
}
