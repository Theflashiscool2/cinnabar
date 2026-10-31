//! Configuration has an explicit dispatch table; unknown required behavior fails closed.
use super::*;
impl Connection {
    pub(super) async fn configure(&mut self) -> Result<(), Error> {
        let mut info = Vec::new();
        wire::string("en_us", &mut info)?;
        // View distance, chat visibility, colors, skin parts, right hand, filtering, listing, particles.
        info.extend([2, 0, 1, 127, 1, 0, 0, 0]);
        self.send("ServerboundClientInformationPacket", &info)
            .await?;
        self.configuration = Configuration::default();
        self.configuration_bytes = 0;
        self.bootstrapped = false;
        loop {
            let packet = self.stream.read().await?;
            self.configuration_bytes = self
                .configuration_bytes
                .checked_add(packet.len())
                .ok_or(ProtocolError::Invalid("configuration size"))?;
            if self.configuration_bytes > 64 * 1024 * 1024 {
                return Err(ProtocolError::Invalid("configuration memory budget exceeded").into());
            }
            let mut r = Reader(&packet);
            let id = r.var()?;
            let name = usize::try_from(id)
                .ok()
                .and_then(|n| target::configuration::clientbound::NAMES.get(n))
                .ok_or(ProtocolError::Invalid("unknown configuration packet"))?;
            if self.common(name, &mut r).await? {
                r.finish()?;
                continue;
            }
            match *name {
                "ClientboundRegistryDataPacket" => {
                    let (name, entries) = facts::registry(&mut r)?;
                    if self
                        .configuration
                        .registries
                        .insert(name, entries)
                        .is_some()
                    {
                        return Err(ProtocolError::Invalid("duplicate registry").into());
                    }
                }
                "ClientboundUpdateEnabledFeaturesPacket" => {
                    self.configuration.features = facts::strings(&mut r)?;
                    if self
                        .configuration
                        .features
                        .iter()
                        .any(|f| f != "minecraft:vanilla")
                    {
                        return Err(Error::Unsupported("experimental feature flags"));
                    }
                }
                "ClientboundSelectKnownPacks" => {
                    let n = r.count(1024)?;
                    for _ in 0..n {
                        r.string(32767)?;
                        r.string(32767)?;
                        r.string(32767)?;
                    }
                    // No local version-matched data pack is installed. Request every registry value.
                    self.send("ServerboundSelectKnownPacks", &[0]).await?;
                }
                "ClientboundFinishConfigurationPacket" => {
                    r.finish()?;
                    if !self
                        .configuration
                        .registries
                        .contains_key("minecraft:dimension_type")
                    {
                        return Err(ProtocolError::Invalid(
                            "configuration finished without dimension registry",
                        )
                        .into());
                    }
                    self.send("ServerboundFinishConfigurationPacket", &[])
                        .await?;
                    self.phase = Phase::Play;
                    self.emit(Event::Phase(self.phase)).await?;
                    return Ok(());
                }
                "ClientboundResetChatPacket" | "ClientboundClearDialogPacket" => {}
                "ClientboundShowDialogConfigurationPacket" => {
                    return Err(Error::Unsupported(
                        "configuration dialogs requiring user interaction",
                    ));
                }
                "ClientboundCodeOfConductPacket" => {
                    return Err(Error::Unsupported(
                        "server code-of-conduct consent (no acceptance sent)",
                    ));
                }
                _ => return Err(Error::Unsupported("configuration packet")),
            }
            r.finish()?;
        }
    }
    /// Handles state-shared packets without crossing their state-specific wire identities.
    pub(super) async fn common(&mut self, name: &str, r: &mut Reader<'_>) -> Result<bool, Error> {
        match name {
            "ClientboundUpdateTagsPacket" => {
                let tags = facts::tags(r)?;
                // Do not publish or mutate authority from a packet with trailing junk.
                Reader(r.0).finish()?;
                if self.bootstrapped {
                    self.emit(Event::Tags(tags)).await?;
                } else {
                    // Also covers play tags sent before the initial world login packet.
                    self.configuration
                        .update_tags(tags)
                        .map_err(ProtocolError::Invalid)?;
                }
            }
            "ClientboundDisconnectPacket" => {
                let reason = crate::nbt::read(r)?;
                return Err(Error::Disconnect(
                    format!("{reason:?}")
                        .chars()
                        .take(2048)
                        .filter(|c| !c.is_control())
                        .collect(),
                ));
            }
            "ClientboundKeepAlivePacket" => {
                let id = r.i64()?;
                self.send("ServerboundKeepAlivePacket", &id.to_be_bytes())
                    .await?;
                self.emit(Event::KeepAlive(id)).await?;
            }
            "ClientboundPingPacket" => {
                let id = r.i32()?;
                self.send("ServerboundPongPacket", &id.to_be_bytes())
                    .await?;
            }
            "ClientboundCookieRequestPacket" => {
                let key = r.string(32767)?;
                self.cookie(&key).await?;
            }
            "ClientboundStoreCookiePacket" => {
                let key = r.string(32767)?;
                let value = r.bytes(5120)?;
                let retained: usize = self
                    .cookies
                    .iter()
                    .filter(|(k, _)| *k != &key)
                    .map(|(k, v)| k.len() + v.len())
                    .sum();
                if retained + key.len() + value.len() > 65536 {
                    return Err(ProtocolError::Invalid("cookie budget").into());
                }
                self.cookies.insert(key, value);
            }
            "ClientboundTransferPacket" => {
                return Err(Error::Unsupported(
                    "server transfer; select the destination explicitly",
                ));
            }
            "ClientboundResourcePackPushPacket" => {
                let id = r.uuid()?;
                let _url = r.string(32767)?;
                let _hash = r.string(40)?;
                let required = r.bool()?;
                if r.bool()? {
                    crate::nbt::read(r)?;
                }
                let mut reply = id.as_bytes().to_vec();
                reply.push(1); // ResourcePackStatus.DECLINED in the pinned codec.
                self.send("ServerboundResourcePackPacket", &reply).await?;
                if required {
                    return Err(Error::Unsupported("required server resource pack"));
                }
            }
            "ClientboundResourcePackPopPacket" => {
                if r.bool()? {
                    r.uuid()?;
                }
            }
            "ClientboundPostEffectsPacket" => {
                facts::strings(r)?;
            }
            "ClientboundCustomPayloadPacket" => {
                let _channel = r.string(32767)?;
                // Vanilla custom payloads are optional extension data, not authority or executable text.
                r.take(r.0.len())?;
            }
            "ClientboundCustomReportDetailsPacket" => {
                let n = r.count(32)?;
                for _ in 0..n {
                    r.string(128)?;
                    r.string(4096)?;
                }
            }
            "ClientboundServerLinksPacket" => {
                let n = r.count(256)?;
                for _ in 0..n {
                    if r.bool()? {
                        r.var()?;
                    } else {
                        crate::nbt::read(r)?;
                    }
                    r.string(32767)?;
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
