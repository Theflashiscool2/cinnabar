//! Version-pinned player chat, commands, signatures and acknowledgements.
#[path = "chat/command_send.rs"]
mod command_send;
#[path = "chat/commands.rs"]
mod commands;
#[path = "chat/player.rs"]
mod player;
#[path = "chat/security.rs"]
pub(crate) mod security;
#[path = "chat/signing.rs"]
mod signing;
#[path = "chat/suggestions.rs"]
pub mod suggestions;
#[path = "chat/window.rs"]
mod window;
use crate::{
    ProtocolError as Error, nbt,
    wire::{self, Reader},
};

/// FriendlyByteBuf.writeUtf/readUtf default used by ServerboundChatCommandPacket.
pub const COMMAND_MAX_UTF16: usize = 32767;
/// Native ChatScreen and player-chat packet UTF-16 bound.
pub const PLAYER_MAX_UTF16: usize = 256;
#[derive(Clone, Debug, serde::Serialize)]
pub struct System {
    pub content: nbt::Value,
    pub overlay: bool,
}
/// Encodes only the command body, without a leading slash or any chat signature.
pub fn command(value: &str) -> Result<Vec<u8>, Error> {
    if value.encode_utf16().count() > COMMAND_MAX_UTF16 {
        return Err(Error::Invalid("Java command exceeds native UTF-16 limit"));
    }
    let mut out = Vec::new();
    wire::string(value, &mut out)?;
    Ok(out)
}
pub(super) fn decode(name: &str, r: &mut Reader<'_>) -> Result<Option<System>, Error> {
    match name {
        "ClientboundSystemChatPacket" => Ok(Some(System {
            content: nbt::read(r)?,
            overlay: r.bool()?,
        })),
        "ClientboundSetActionBarTextPacket" => Ok(Some(System {
            content: nbt::read(r)?,
            overlay: true,
        })),
        _ => Ok(None),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_26_3_stream_vectors_and_all_truncations() {
        for (value, expected) in [
            ("help", b"\x04help".as_slice()),
            ("time query daytime", b"\x12time query daytime"),
            ("say 夜", b"\x07say \xe5\xa4\x9c"),
        ] {
            assert_eq!(command(value).unwrap(), expected);
        }
        for (bytes, overlay) in [
            (b"\x08\x00\x05Hello\x00".as_slice(), false),
            (b"\x08\x00\x05Ready\x01", true),
        ] {
            let mut r = Reader(bytes);
            let value = decode("ClientboundSystemChatPacket", &mut r)
                .unwrap()
                .unwrap();
            assert_eq!(value.overlay, overlay);
            r.finish().unwrap();
            for end in 0..bytes.len() {
                assert!(decode("ClientboundSystemChatPacket", &mut Reader(&bytes[..end])).is_err());
            }
        }
        let actionbar = b"\x08\x00\x05Ready";
        let mut r = Reader(actionbar);
        assert!(
            decode("ClientboundSetActionBarTextPacket", &mut r)
                .unwrap()
                .unwrap()
                .overlay
        );
        r.finish().unwrap();
        for end in 0..actionbar.len() {
            assert!(
                decode(
                    "ClientboundSetActionBarTextPacket",
                    &mut Reader(&actionbar[..end])
                )
                .is_err()
            );
        }
        assert!(command(&"x".repeat(COMMAND_MAX_UTF16)).is_ok());
        assert!(command(&"x".repeat(COMMAND_MAX_UTF16 + 1)).is_err());
        assert!(
            command(&"😀".repeat(16384)).is_err(),
            "limit counts UTF-16, not Unicode scalars"
        );
    }
}

/// Connection-local receive cache and acknowledgement window; never shared between accounts.
#[derive(Default)]
pub(super) struct State {
    pub suggestions: suggestions::Pending,
    pub commands: Option<commands::Tree>,
    pub command_permission: u8,
    pub command_entity: Option<i32>,
    window: window::Window,
    cache: window::Cache,
    next_index: i32,
    pub secure: bool,
    roots: Option<security::Roots>,
    local: Option<signing::Local>,
    cached_certificate: Option<std::sync::Arc<crate::auth::chat::Certificate>>,
    online: bool,
    remotes: signing::Remotes,
}
impl State {
    pub fn install(
        &mut self,
        material: crate::auth::chat::Material,
        now: i64,
    ) -> Result<Option<Vec<u8>>, Error> {
        self.remotes.install_roots(&material.roots);
        self.roots = Some(material.roots);
        let mut changed = false;
        if let Some(certificate) = material.certificate
            && certificate.expires_at >= now
        {
            let same = self.cached_certificate.as_ref().is_some_and(|old| {
                old.owner == certificate.owner
                    && old.expires_at == certificate.expires_at
                    && old.refreshed_after == certificate.refreshed_after
                    && old.public_der == certificate.public_der
                    && old.signature == certificate.signature
            });
            if same {
                return Ok(None);
            }
            changed = true;
            let certificate = std::sync::Arc::new(certificate);
            self.cached_certificate = Some(std::sync::Arc::clone(&certificate));
            if self.online {
                self.local = Some(signing::Local::new(certificate));
            }
        }
        if self.online && changed {
            self.local
                .as_ref()
                .map(signing::Local::announcement)
                .transpose()
        } else {
            Ok(None)
        }
    }

    pub fn observe_roster(&mut self, packet: &super::roster::Packet) {
        self.remotes.observe(packet, self.roots.as_ref());
    }
    pub fn online(&self) -> bool {
        self.online
    }
    pub fn needs_preparation(&self, now: i64) -> bool {
        self.roots.is_none()
            || self.online
                && self
                    .cached_certificate
                    .as_ref()
                    .is_none_or(|c| c.refreshed_after <= now || c.expires_at < now)
    }
    pub fn has_roots(&self) -> bool {
        self.roots.is_some()
    }
    pub fn signing_ready(&self) -> bool {
        self.local.is_some()
    }
    pub fn can_send(&self) -> bool {
        !self.secure || self.local.is_some()
    }
    pub fn outgoing(&mut self, text: &str, timestamp: i64, salt: i64) -> Result<Vec<u8>, Error> {
        if self.local.is_none() {
            if self.secure {
                return Err(Error::Invalid(
                    "signed Java chat certificate is unavailable",
                ));
            }
            return self.unsigned(text, timestamp, salt);
        }
        let mut out = Vec::new();
        crate::codec::write_string(text, PLAYER_MAX_UTF16, &mut out)?;
        let mut pending_window = self.window.clone();
        let (update, last_seen) = pending_window.update_with_signatures();
        let signature = self
            .local
            .as_mut()
            .unwrap()
            .sign(text, timestamp, salt, &last_seen)?;
        out.extend(timestamp.to_be_bytes());
        out.extend(salt.to_be_bytes());
        out.push(1);
        out.extend(signature);
        out.extend(update);
        self.window = pending_window;
        Ok(out)
    }
    /// Native handleLogin retires chat counters/session even on an existing play connection.
    pub fn world_login(
        &mut self,
        online: bool,
        secure: bool,
        now: i64,
    ) -> Result<Option<Vec<u8>>, Error> {
        let commands = self.commands.take();
        let roots = self.roots.take();
        let remotes = std::mem::take(&mut self.remotes);
        let cached_certificate = self.cached_certificate.take();
        *self = Self {
            commands,
            roots,
            remotes,
            cached_certificate,
            online,
            secure,
            ..Default::default()
        };
        if online {
            self.local = self
                .cached_certificate
                .as_ref()
                .filter(|c| c.expires_at >= now)
                .cloned()
                .map(signing::Local::new);
        }
        self.local
            .as_ref()
            .map(signing::Local::announcement)
            .transpose()
    }
    pub fn reconfigure(&mut self) -> Result<Option<Vec<u8>>, Error> {
        self.commands = None;
        self.remotes = Default::default();
        self.world_login(false, false, 0)
    }
    pub fn unsigned(&mut self, text: &str, timestamp: i64, salt: i64) -> Result<Vec<u8>, Error> {
        if text.encode_utf16().count() > PLAYER_MAX_UTF16 {
            return Err(Error::Invalid("Java chat exceeds256 UTF-16 units"));
        }
        let mut out = Vec::new();
        crate::codec::write_string(text, PLAYER_MAX_UTF16, &mut out)?;
        out.extend_from_slice(&timestamp.to_be_bytes());
        out.extend_from_slice(&salt.to_be_bytes());
        out.push(0); // Legitimately absent signature, never an invented signature.
        out.extend(self.window.update());
        Ok(out)
    }
    pub fn receive(
        &mut self,
        name: &str,
        r: &mut Reader<'_>,
        configuration: &super::facts::Configuration,
    ) -> Result<Option<System>, Error> {
        let registry = configuration
            .registries
            .get("minecraft:chat_type")
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        match name {
            "ClientboundDisguisedChatPacket" => player::disguised(r, registry).map(Some),
            "ClientboundPlayerChatPacket" => {
                let message = player::player(r, &self.cache, registry)?;
                if message.global != self.next_index {
                    return Err(Error::Invalid("out-of-order Java player chat"));
                }
                self.next_index = self
                    .next_index
                    .checked_add(1)
                    .ok_or(Error::Invalid("Java chat index exhausted"))?;
                self.cache.push(&message.last_seen, message.signature);
                let verified = self.remotes.verify(
                    &message,
                    crate::auth::chat::now_millis_protocol()?,
                    self.secure,
                );
                if let Some(signature) = message.signature {
                    if !verified {
                        self.window.observe(signature, false);
                        return Ok(Some(System{content:nbt::Value::String("A signed Java chat message could not be verified; its player certificate or signed chain is unavailable or invalid.".into()),overlay:false}));
                    }
                    self.window.observe(signature, message.content.is_some());
                } else if !verified {
                    return Ok(Some(System {
                        content: nbt::Value::String(
                            "Java player chat did not match the authenticated sender session."
                                .into(),
                        ),
                        overlay: false,
                    }));
                }
                message
                    .content
                    .map(|content| message.bound.decorate(content))
                    .transpose()
            }
            "ClientboundDeleteChatPacket" => {
                let signature = player::signature(r, &self.cache)?;
                self.window.ignore(&signature);
                Ok(None)
            }
            _ => Ok(None),
        }
    }
    pub fn acknowledgement(&mut self) -> Option<Vec<u8>> {
        if self.window.offset() <= 64 {
            return None;
        }
        self.flush_acknowledgement()
    }
    pub fn flush_acknowledgement(&mut self) -> Option<Vec<u8>> {
        if self.window.offset() <= 0 {
            return None;
        }
        let mut bytes = Vec::new();
        crate::codec::write_varint(self.window.take_offset(), &mut bytes);
        Some(bytes)
    }
}

#[cfg(test)]
mod player_tests {
    use super::*;
    fn bytes(hex: &str) -> Vec<u8> {
        hex.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn native_stream_vectors_and_truncations() {
        let configuration = super::super::facts::Configuration::default();
        let b = bytes(
            "000000000000000000000000000000000100000548656c6c6f0000018bcfe5687b000000000000002a000000000e636861742e747970652e746578740200020a000e636861742e747970652e746578740200020a000800045065657200",
        );
        let mut state = State::default();
        let mut r = Reader(&b);
        let message = state
            .receive("ClientboundPlayerChatPacket", &mut r, &configuration)
            .unwrap()
            .unwrap();
        r.finish().unwrap();
        let nbt::Value::Compound(parts) = message.content else {
            panic!("decoration")
        };
        assert_eq!(
            parts.get("translate"),
            Some(&nbt::Value::String("chat.type.text".into()))
        );
        let Some(nbt::Value::List(args)) = parts.get("with") else {
            panic!("arguments")
        };
        assert_eq!(
            args,
            &vec![
                nbt::Value::String("Peer".into()),
                nbt::Value::String("Hello".into())
            ]
        );
        for end in 0..b.len() {
            assert!(
                State::default()
                    .receive(
                        "ClientboundPlayerChatPacket",
                        &mut Reader(&b[..end]),
                        &configuration
                    )
                    .is_err(),
                "truncation {end}"
            );
        }
        let b = bytes(
            "000000000000000000000000000000000100000548656c6c6f0000018bcfe5687b000000000000002a00010800084f7665727269646500000e636861742e747970652e746578740200020a000e636861742e747970652e746578740200020a000800045065657200",
        );
        let mut state = State::default();
        let mut r = Reader(&b);
        let message = state
            .receive("ClientboundPlayerChatPacket", &mut r, &configuration)
            .unwrap()
            .unwrap();
        r.finish().unwrap();
        let nbt::Value::Compound(parts) = message.content else {
            panic!("decoration")
        };
        assert_eq!(
            parts.get("translate"),
            Some(&nbt::Value::String("chat.type.text".into()))
        );
        let Some(nbt::Value::List(args)) = parts.get("with") else {
            panic!("arguments")
        };
        assert_eq!(
            args,
            &vec![
                nbt::Value::String("Peer".into()),
                nbt::Value::String("Override".into())
            ]
        );
        for end in 0..b.len() {
            assert!(
                State::default()
                    .receive(
                        "ClientboundPlayerChatPacket",
                        &mut Reader(&b[..end]),
                        &configuration
                    )
                    .is_err(),
                "truncation {end}"
            );
        }
        let b = bytes(
            "08000548656c6c6f000e636861742e747970652e746578740200020a000e636861742e747970652e746578740200020a000800045065657200",
        );
        let mut state = State::default();
        let mut r = Reader(&b);
        let message = state
            .receive("ClientboundDisguisedChatPacket", &mut r, &configuration)
            .unwrap()
            .unwrap();
        r.finish().unwrap();
        let nbt::Value::Compound(parts) = message.content else {
            panic!("decoration")
        };
        assert_eq!(
            parts.get("translate"),
            Some(&nbt::Value::String("chat.type.text".into()))
        );
        let Some(nbt::Value::List(args)) = parts.get("with") else {
            panic!("arguments")
        };
        assert_eq!(
            args,
            &vec![
                nbt::Value::String("Peer".into()),
                nbt::Value::String("Hello".into())
            ]
        );
        for end in 0..b.len() {
            assert!(
                State::default()
                    .receive(
                        "ClientboundDisguisedChatPacket",
                        &mut Reader(&b[..end]),
                        &configuration
                    )
                    .is_err(),
                "truncation {end}"
            );
        }
        assert_eq!(
            State::default()
                .unsigned("Hello", 1700000000123, 42)
                .unwrap(),
            bytes("0548656c6c6f0000018bcfe5687b000000000000002a000000000001")
        );
    }
    #[test]
    fn rejects_missing_references_and_out_of_order_messages() {
        let mut s = State::default();
        let c = super::super::facts::Configuration::default();
        let mut b = bytes(
            "000000000000000000000000000000000100000548656c6c6f0000018bcfe5687b000000000000002a000000000e636861742e747970652e746578740200020a000e636861742e747970652e746578740200020a000800045065657200",
        );
        b[0] = 1;
        assert!(
            s.receive("ClientboundPlayerChatPacket", &mut Reader(&b), &c)
                .is_err()
        );
        assert!(player::signature(&mut Reader(&[1]), &window::Cache::default()).is_err());
        assert!(s.unsigned(&"😀".repeat(129), 0, 0).is_err());
    }
    #[test]
    fn unverified_signatures_are_not_acknowledged_as_displayed() {
        let c = super::super::facts::Configuration::default();
        let template = bytes(
            "000000000000000000000000000000000100000548656c6c6f0000018bcfe5687b000000000000002a000000000e636861742e747970652e746578740200020a000e636861742e747970652e746578740200020a000800045065657200",
        );
        let mut s = State::default();
        for index in 0..65 {
            let mut b = template.clone();
            b[0] = index;
            b[18] = 1;
            let mut signature = [0; 256];
            signature[0] = index;
            b.splice(19..19, signature);
            let reply = s
                .receive("ClientboundPlayerChatPacket", &mut Reader(&b), &c)
                .unwrap()
                .unwrap();
            assert!(
                matches!(reply.content,nbt::Value::String(ref text) if text.contains("could not be verified"))
            );
            if index < 64 {
                assert!(s.acknowledgement().is_none());
            }
        }
        assert_eq!(s.acknowledgement(), Some(vec![65]));
        let bytes = s.unsigned("reply", 0, 0).unwrap();
        assert_eq!(
            &bytes[bytes.len() - 5..],
            &[0, 0, 0, 0, 1],
            "no seen bits for undisplayed content"
        );
    }
}

#[cfg(test)]
#[path = "chat/security_tests.rs"]
mod security_tests;

impl super::Connection {
    /// Native handleConfigurationStart flushes queued chat and then its remaining offset,
    /// before changing packet phase. Unverifiable messages remain explicitly not displayed.
    pub(super) async fn flush_chat_for_configuration(&mut self) -> Result<(), super::Error> {
        while let Some(frame) = self.chat_pending.pop() {
            let mut r = Reader(&frame.bytes);
            let id = r.var()?;
            let name = usize::try_from(id)
                .ok()
                .and_then(|id| crate::target::game::clientbound::NAMES.get(id))
                .ok_or(Error::Invalid("queued Java chat packet id"))?;
            if let Some(update) = super::roster::decode(name, &mut r)? {
                r.finish()?;
                self.chat.observe_roster(&update);
            } else {
                let message = self.chat.receive(name, &mut r, &self.configuration)?;
                r.finish()?;
                if let Some(message) = message {
                    self.emit(super::Event::SystemChat(message)).await?;
                }
                if let Some(bytes) = self.chat.acknowledgement() {
                    self.send("ServerboundChatAckPacket", &bytes).await?;
                }
            }
        }
        if let Some(bytes) = self.chat.flush_acknowledgement() {
            self.send("ServerboundChatAckPacket", &bytes).await?;
        }
        Ok(())
    }
}

impl super::Connection {
    pub(super) async fn command_tree(&mut self, r: &mut Reader<'_>) -> Result<(), super::Error> {
        let tree = commands::Tree::decode(r)?;
        std::mem::replace(r, Reader(&[])).finish()?;
        let (nodes, message_arguments) = tree.counts();
        self.chat.commands = Some(tree);
        self.emit(super::Event::CommandTree {
            nodes,
            message_arguments,
        })
        .await
    }
    pub(super) async fn command_permission(
        &mut self,
        id: i32,
        event: u8,
    ) -> Result<(), super::Error> {
        if self.chat.command_entity == Some(id)
            && let Some(level) = permission_level(event)
        {
            self.chat.command_permission = level;
            self.emit(super::Event::CommandPermission(level)).await?;
        }
        Ok(())
    }
}

/// EntityEvent.PERMISSION_LEVEL_ALL..PERMISSION_LEVEL_OWNERS in the verified26.3 archive.
pub(crate) fn permission_level(event: u8) -> Option<u8> {
    const FIRST: u8 = 24;
    (FIRST..=FIRST + 4).contains(&event).then(|| event - FIRST)
}
