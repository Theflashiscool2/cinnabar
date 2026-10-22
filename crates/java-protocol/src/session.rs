//! Native login/configuration/play owner. One future owns all ordered socket operations.
mod configuration;
mod crypto;
pub mod facts;
mod login;
mod play;
mod terrain;
use crate::{
    ProtocolError,
    address::{self, Address},
    auth, codec,
    stream::Stream,
    target,
    wire::{self, Reader},
};
use facts::{Bootstrap, Configuration};
use std::{collections::BTreeMap, time::Duration};
use tokio::{net::TcpStream, sync::mpsc};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Phase {
    Connecting,
    Login,
    Configuration,
    Play,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Transport(#[from] crate::stream::Error),
    #[error("Java connection I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Authentication(#[from] auth::Error),
    #[error("Java connection cancelled")]
    Cancelled,
    #[error("Java connection deadline expired during {0:?}")]
    Timeout(Phase),
    #[error("Java server disconnected: {0}")]
    Disconnect(String),
    #[error("Java {0} is not supported by this connection foundation")]
    Unsupported(&'static str),
    #[error("Java session consumer closed")]
    ConsumerClosed,
}
/// Offline identities are allowed only when the caller explicitly requests a loopback fixture.
pub enum Identity {
    Online(auth::Account),
    OfflineFixture { name: String },
}
/// User intent for this connection only; transport replies do not enter this queue.
#[derive(Debug)]
pub enum Command {
    Respawn,
    /// Sent only after the active dimension has actually reached renderer readiness.
    PlayerLoaded {
        dimension_epoch: u64,
    },
}
pub struct Options {
    pub address: Address,
    pub identity: Identity,
    pub generation: u64,
    pub connect_timeout: Duration,
    pub login_timeout: Duration,
    pub commands: Option<mpsc::Receiver<Command>>,
}
impl Options {
    pub fn fixture(address: Address, name: String, generation: u64) -> Self {
        Self {
            address,
            identity: Identity::OfflineFixture { name },
            generation,
            connect_timeout: Duration::from_secs(10),
            login_timeout: Duration::from_secs(30),
            commands: None,
        }
    }
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum Event {
    Phase(Phase),
    Bootstrap(Box<Bootstrap>),
    Teleport(facts::Teleport),
    Respawn(facts::Spawn),
    KeepAlive(i64),
    Vitals(facts::Vitals),
    Death {
        player_id: i32,
        message: crate::nbt::Value,
    },
    Terrain {
        dimension_epoch: u64,
        packet: Box<crate::terrain::Packet>,
    },
    /// Known wire packets not yet admitted to gameplay; diagnostics retain the distinction.
    DeferredPlay {
        name: String,
        bytes: Vec<u8>,
    },
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Envelope {
    pub generation: u64,
    pub event: Event,
}
struct Connection {
    stream: Stream<TcpStream>,
    phase: Phase,
    generation: u64,
    events: mpsc::Sender<Envelope>,
    configuration: Configuration,
    profile: auth::Profile,
    server_session: uuid::Uuid,
    cookies: BTreeMap<String, Vec<u8>>,
    configuration_bytes: usize,
    bootstrapped: bool,
    terrain: Option<crate::terrain::Context>,
    dimensions: Vec<crate::terrain::Context>,
    dimension_epoch: u64,
    commands: Option<mpsc::Receiver<Command>>,
    dead: bool,
    loaded_epoch: Option<u64>,
}
/// Runs until cancelled, disconnected, or a required behavior is unsupported. Cancelling drops
/// every in-flight read/write and its socket. Login deadlines do not limit the play lifetime.
pub async fn run(
    options: Options,
    events: mpsc::Sender<Envelope>,
    cancel: CancellationToken,
) -> Result<(), Error> {
    tokio::select! {biased;_=cancel.cancelled()=>Err(Error::Cancelled),result=run_inner(options,events)=>result}
}
async fn run_inner(mut options: Options, events: mpsc::Sender<Envelope>) -> Result<(), Error> {
    let profile = match &options.identity {
        Identity::Online(account) => {
            if !account.valid_at(auth::unix_now()) {
                return Err(auth::Error::Expired.into());
            }
            account.profile.clone()
        }
        Identity::OfflineFixture { name } => {
            if name.is_empty()
                || name.len() > 16
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(ProtocolError::Invalid("fixture username").into());
            }
            auth::Profile {
                name: name.clone(),
                id: offline_uuid(name),
            }
        }
    };
    events
        .send(Envelope {
            generation: options.generation,
            event: Event::Phase(Phase::Connecting),
        })
        .await
        .map_err(|_| Error::ConsumerClosed)?;
    let socket = tokio::time::timeout(options.connect_timeout, async {
        let sockets = address::resolve(&options.address).await?;
        if matches!(options.identity, Identity::OfflineFixture { .. })
            && sockets.iter().any(|s| !s.ip().is_loopback())
        {
            return Err(std::io::Error::other(
                "offline fixture identity is restricted to loopback",
            ));
        }
        TcpStream::connect(sockets.as_slice()).await
    })
    .await
    .map_err(|_| Error::Timeout(Phase::Connecting))??;
    socket.set_nodelay(true)?;
    let mut c = Connection {
        stream: Stream::new(socket),
        phase: Phase::Login,
        generation: options.generation,
        events,
        configuration: Configuration::default(),
        profile,
        server_session: uuid::Uuid::nil(),
        cookies: BTreeMap::new(),
        configuration_bytes: 0,
        bootstrapped: false,
        terrain: None,
        dimensions: Vec::new(),
        dimension_epoch: 0,
        commands: options.commands.take(),
        dead: false,
        loaded_epoch: None,
    };
    tokio::time::timeout(options.login_timeout, c.login(&options))
        .await
        .map_err(|_| Error::Timeout(c.phase))??;
    c.play().await
}
impl Connection {
    async fn emit(&self, event: Event) -> Result<(), Error> {
        self.events
            .send(Envelope {
                generation: self.generation,
                event,
            })
            .await
            .map_err(|_| Error::ConsumerClosed)
    }
    async fn send(&mut self, name: &str, payload: &[u8]) -> Result<(), Error> {
        let names = match self.phase {
            Phase::Login => target::login::serverbound::NAMES,
            Phase::Configuration => target::configuration::serverbound::NAMES,
            Phase::Play => target::game::serverbound::NAMES,
            Phase::Connecting => target::handshake::serverbound::NAMES,
        };
        let mut body = Vec::new();
        codec::write_varint(target::id(names, name), &mut body);
        body.extend_from_slice(payload);
        self.stream.write(&body).await?;
        Ok(())
    }
    async fn cookie(&mut self, key: &str) -> Result<(), Error> {
        let mut p = Vec::new();
        wire::string(key, &mut p)?;
        if let Some(value) = self.cookies.get(key) {
            p.push(1);
            wire::bytes(value, &mut p)?;
        } else {
            p.push(0);
        }
        self.send("ServerboundCookieResponsePacket", &p).await
    }
}

/// Java offline UUID uses raw MD5 name bytes, without an RFC namespace prefix.
fn offline_uuid(name: &str) -> uuid::Uuid {
    use md5::{Digest, Md5};
    uuid::Builder::from_md5_bytes(Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into())
        .into_uuid()
}
