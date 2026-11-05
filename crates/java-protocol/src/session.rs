//! Native login/configuration/play owner. One future owns all ordered socket operations.
pub mod chat;
mod configuration;
mod crypto;
pub mod dynamics;
pub mod entities;
mod events;
pub mod facts;
pub mod hud;
pub mod interaction;
pub mod inventory;
pub use events::{Envelope, Event};
mod ingress;
mod login;
mod motion;
pub mod movement;
mod play;
pub mod roster;
pub mod scoreboard;
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
    #[error("Java play packet {packet} could not be decoded: {source}")]
    PlayPacket {
        packet: &'static str,
        #[source]
        source: ProtocolError,
    },
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
/// User intent and admitted player authority for this connection only.
#[derive(Debug)]
pub enum Command {
    CommandSuggestions {
        dimension_epoch: u64,
        id: i32,
        input: String,
    },
    ChatMessage {
        dimension_epoch: u64,
        sequence: u64,
        text: String,
    },
    ChatCommand {
        dimension_epoch: u64,
        sequence: u64,
        text: String,
    },
    /// Ordered game-thread receipt; unlike keep-alive, a pong cannot overtake world admission.
    Pong {
        sequence: u64,
        id: i32,
    },
    Attack {
        dimension_epoch: u64,
        correction_sequence: u64,
        entity: i32,
        lifetime: u64,
    },
    ContainerClick {
        dimension_epoch: u64,
        incarnation: u64,
        sequence: u64,
        click: inventory::outbound::PreparedClick,
    },
    ContainerClose {
        dimension_epoch: u64,
        incarnation: u64,
        sequence: u64,
        container: i32,
    },
    /// Slot range is admitted by the inventory owner. Sequence identifies the write receipt.
    Select {
        dimension_epoch: u64,
        sequence: u64,
        slot: u8,
    },
    Interact {
        dimension_epoch: u64,
        correction_sequence: u64,
        action: interaction::Dig,
    },
    UseOn {
        dimension_epoch: u64,
        correction_sequence: u64,
        action: interaction::UseOn,
    },
    UseEntity {
        dimension_epoch: u64,
        correction_sequence: u64,
        lifetime: u64,
        action: interaction::EntityUse,
    },
    UseItem {
        dimension_epoch: u64,
        correction_sequence: u64,
        action: interaction::UseItem,
    },
    Move {
        dimension_epoch: u64,
        sequence: u64,
        frame: movement::Frame,
    },
    Respawn,
    AcceptRotation {
        dimension_epoch: u64,
        sequence: u64,
        yaw: f32,
        pitch: f32,
    },
    /// Sent only after the active dimension has actually reached renderer readiness.
    PlayerLoaded {
        dimension_epoch: u64,
    },
    /// The resolved player position, after the domain owner applies relative correction fields.
    AcceptTeleport {
        dimension_epoch: u64,
        sequence: u64,
        id: i32,
        position: [f64; 3],
        yaw: f32,
        pitch: f32,
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
struct Connection {
    stream: Stream<TcpStream>,
    phase: Phase,
    generation: u64,
    events: mpsc::Sender<Envelope>,
    ingress: ingress::Ingress,
    configuration: Configuration,
    chat: chat::State,
    chat_account: Option<std::sync::Arc<auth::Account>>,
    chat_retry_after: i64,
    chat_preparation: Option<
        std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<auth::chat::Material, auth::Error>> + Send>,
        >,
    >,
    chat_announcement: Option<Vec<u8>>,
    chat_pending: ingress::Ingress,
    profile: auth::Profile,
    server_session: uuid::Uuid,
    cookies: BTreeMap<String, Vec<u8>>,
    configuration_bytes: usize,
    configuration_timeout: Duration,
    bootstrapped: bool,
    terrain: Option<crate::terrain::Context>,
    dimensions: Vec<crate::terrain::Context>,
    dimension_epoch: u64,
    commands: Option<mpsc::Receiver<Command>>,
    dead: bool,
    loaded_epoch: Option<u64>,
    cancelled_use: interaction::cancel::Pending,
    menu_lifetime: inventory::lifetime::Lifetime,
    entity_lifetimes: entities::lifetime::Lifetimes,
    pending_teleport: Option<(i32, u64)>,
    pending_rotation: Option<u64>,
    pending_ping: Option<(u64, i32)>,
    ping_sequence: u64,
    correction_sequence: u64,
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
        ingress: ingress::Ingress::new(crate::stream::MAX_UNCOMPRESSED),
        configuration: Configuration::default(),
        chat: Default::default(),
        chat_account: None,
        chat_retry_after: 0,
        chat_preparation: None,
        chat_announcement: None,
        chat_pending: ingress::Ingress::new(crate::stream::MAX_UNCOMPRESSED),
        profile,
        server_session: uuid::Uuid::nil(),
        cookies: BTreeMap::new(),
        configuration_bytes: 0,
        configuration_timeout: options.login_timeout,
        bootstrapped: false,
        terrain: None,
        dimensions: Vec::new(),
        dimension_epoch: 0,
        menu_lifetime: Default::default(),
        entity_lifetimes: Default::default(),
        commands: options.commands.take(),
        dead: false,
        loaded_epoch: None,
        cancelled_use: Default::default(),
        pending_teleport: None,
        pending_rotation: None,
        pending_ping: None,
        ping_sequence: 0,
        correction_sequence: 0,
    };
    tokio::time::timeout(options.login_timeout, c.login(&options))
        .await
        .map_err(|_| Error::Timeout(c.phase))??;
    if let Identity::Online(account) = options.identity {
        c.chat_account = Some(std::sync::Arc::new(account));
    }
    c.play().await
}
impl Connection {
    async fn emit(&mut self, event: Event) -> Result<(), Error> {
        let envelope = Envelope {
            generation: self.generation,
            event,
        };
        let envelope = match self.events.try_send(envelope) {
            Ok(()) => return Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => return Err(Error::ConsumerClosed),
            Err(mpsc::error::TrySendError::Full(envelope)) => envelope,
        };
        let events = self.events.clone();
        loop {
            tokio::select! {
                permit = events.reserve() => {
                    permit.map_err(|_| Error::ConsumerClosed)?.send(envelope);
                    return Ok(());
                },
                packet = self.stream.read(), if matches!(self.phase, Phase::Configuration | Phase::Play) && self.ingress.can_read() => {
                    let mut frame = ingress::Frame::new(packet?);
                    let mut reader = Reader(&frame.bytes);
                    let id = reader.var()?;
                    let names = if self.phase == Phase::Play { target::game::clientbound::NAMES } else { target::configuration::clientbound::NAMES };
                    let name = usize::try_from(id).ok().and_then(|id| names.get(id)).copied();
                    if name == Some("ClientboundDisconnectPacket") {
                        return Err(Self::disconnect_reason(&mut reader)?);
                    }
                    if name == Some("ClientboundKeepAlivePacket") {
                        let id = reader.i64()?;
                        reader.finish()?;
                        self.send("ServerboundKeepAlivePacket", &id.to_be_bytes()).await?;
                        frame.keepalive_replied = true;
                    }
                    // Never decode ahead across a protocol-state/codec boundary. The queued
                    // frame must first be consumed by its actual state owner.
                    frame.boundary = matches!(name, Some("ClientboundFinishConfigurationPacket" | "ClientboundStartConfigurationPacket"));
                    self.ingress.push(frame);
                }
            }
        }
    }
    async fn read_frame(&mut self) -> Result<ingress::Frame, Error> {
        if let Some(frame) = self.ingress.pop() {
            return Ok(frame);
        }
        Ok(ingress::Frame::new(self.stream.read().await?))
    }
    fn disconnect_reason(reader: &mut Reader<'_>) -> Result<Error, Error> {
        let reason = crate::nbt::read(reader)?;
        Reader(reader.0).finish()?;
        Ok(Error::Disconnect(
            format!("{reason:?}")
                .chars()
                .take(2048)
                .filter(|c| !c.is_control())
                .collect(),
        ))
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
