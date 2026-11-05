//! Configuration has an explicit dispatch table; unknown required behavior fails closed.
use super::*;
impl Connection {
    /// Reuse the authenticated encrypted stream and cookie store; retire play authority.
    /// Native reference net.rs at 59d8cc38a2627f5b7aa0c96fc91b013df70c45fc (MIT).
    pub(super) async fn reconfigure(&mut self) -> Result<(), Error> {
        tracing::info!(target: "java_session", generation=self.generation, dimension_epoch=self.dimension_epoch, "Java server requested configuration handoff; retiring play authority");
        let started = std::time::Instant::now();
        self.chat = Default::default();
        self.bootstrapped = false;
        self.terrain = None;
        self.dimensions.clear();
        self.loaded_epoch = None;
        self.cancelled_use.clear();
        self.pending_teleport = None;
        self.pending_rotation = None;
        self.pending_ping = None;
        self.menu_lifetime.retire();
        self.entity_lifetimes.retire();
        self.send("ServerboundConfigurationAcknowledgedPacket", &[])
            .await?;
        self.phase = Phase::Configuration;
        self.emit(Event::Phase(self.phase)).await?;
        tokio::time::timeout(self.configuration_timeout, self.configure())
            .await
            .map_err(|_| Error::Timeout(Phase::Configuration))??;
        tracing::info!(target: "java_session", generation=self.generation, dimension_epoch=self.dimension_epoch, registries=self.configuration.registries.len(), elapsed_ms=started.elapsed().as_millis(), "Java configuration handoff completed; awaiting authoritative world login");
        Ok(())
    }

    pub(super) async fn configure(&mut self) -> Result<(), Error> {
        let mut info = Vec::new();
        wire::string("en_us", &mut info)?;
        // View distance, chat visibility, colors, skin parts, right hand, filtering, listing, particles.
        info.extend([2, 0, 1, 127, 1, 0, 0, 0]);
        self.send("ServerboundClientInformationPacket", &info)
            .await?;
        // Native RegistryDataCollector reuses receivedRegistries when no new contents arrive.
        // A full contents collection replaces the registry set; tag-only updates retain it.
        let previous = std::mem::take(&mut self.configuration);
        let mut received_features = false;
        self.configuration_bytes = 0;
        self.bootstrapped = false;
        loop {
            let packet = self.read_frame().await?;
            self.configuration_bytes = self
                .configuration_bytes
                .checked_add(packet.bytes.len())
                .ok_or(ProtocolError::Invalid("configuration size"))?;
            if self.configuration_bytes > 64 * 1024 * 1024 {
                return Err(ProtocolError::Invalid("configuration memory budget exceeded").into());
            }
            let mut r = Reader(&packet.bytes);
            let id = r.var()?;
            let name = usize::try_from(id)
                .ok()
                .and_then(|n| target::configuration::clientbound::NAMES.get(n))
                .ok_or(ProtocolError::Invalid("unknown configuration packet"))?;
            if self.common(name, &mut r, packet.keepalive_replied).await? {
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
                    received_features = true;
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
                    if self.configuration.registries.is_empty() {
                        self.configuration.registries = previous.registries;
                        let updates = std::mem::take(&mut self.configuration.tags);
                        self.configuration.tags = previous.tags;
                        self.configuration
                            .update_tags(updates)
                            .map_err(ProtocolError::Invalid)?;
                    }
                    if !received_features {
                        self.configuration.features = previous.features;
                    }
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
    pub(super) async fn common(
        &mut self,
        name: &str,
        r: &mut Reader<'_>,
        keepalive_replied: bool,
    ) -> Result<bool, Error> {
        match name {
            "ClientboundUpdateTagsPacket" => {
                let tags = facts::tags(r)?;
                // Do not publish or mutate authority from a packet with trailing junk.
                Reader(r.0).finish()?;
                if self.bootstrapped {
                    self.configuration
                        .update_tags(tags.clone())
                        .map_err(ProtocolError::Invalid)?;
                    self.emit(Event::Tags(tags)).await?;
                } else {
                    // Also covers play tags sent before the initial world login packet.
                    self.configuration
                        .update_tags(tags)
                        .map_err(ProtocolError::Invalid)?;
                }
            }
            "ClientboundDisconnectPacket" => {
                return Err(Self::disconnect_reason(r)?);
            }
            "ClientboundKeepAlivePacket" => {
                let id = r.i64()?;
                Reader(r.0).finish()?;
                if !keepalive_replied {
                    self.send("ServerboundKeepAlivePacket", &id.to_be_bytes())
                        .await?;
                }
                self.emit(Event::KeepAlive(id)).await?;
            }
            "ClientboundPingPacket" => {
                let id = r.i32()?;
                Reader(r.0).finish()?;
                if self.phase == Phase::Play {
                    self.ping_sequence = self
                        .ping_sequence
                        .checked_add(1)
                        .ok_or(ProtocolError::Invalid("Java ping sequence overflow"))?;
                    self.pending_ping = Some((self.ping_sequence, id));
                    self.emit(Event::Ping {
                        sequence: self.ping_sequence,
                        id,
                    })
                    .await?;
                } else {
                    self.send("ServerboundPongPacket", &id.to_be_bytes())
                        .await?;
                }
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
