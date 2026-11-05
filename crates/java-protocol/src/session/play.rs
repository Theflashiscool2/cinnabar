//! Essential play bootstrap and acknowledgements. Deferred packets are never gameplay facts.
use super::*;
impl Connection {
    pub(super) async fn play(&mut self) -> Result<(), Error> {
        let mut deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            let now = auth::chat::now_millis_protocol()?;
            if self.bootstrapped
                && self.chat_preparation.is_none()
                && self.chat.needs_preparation(now)
                && now >= self.chat_retry_after
                && let Some(account) = &self.chat_account
            {
                self.chat_retry_after =
                    now.saturating_add(auth::chat::MINIMUM_REFRESH_INTERVAL_MILLIS);
                self.chat_preparation = Some(Box::pin(auth::chat::prepare(
                    std::sync::Arc::clone(account),
                    self.chat.online(),
                )));
            }
            if self.bootstrapped
                && let Some(bytes) = self.chat_announcement.take()
            {
                self.send("ServerboundChatSessionUpdatePacket", &bytes)
                    .await?;
            }
            let awaiting_authority = self.pending_teleport.is_some()
                || self.pending_rotation.is_some()
                || self.pending_ping.is_some();
            let preparing_chat = self.chat_preparation.is_some() && !self.chat.has_roots();
            let (packet, chat_replay) = tokio::select! {
                result = async {
                    if !awaiting_authority && !preparing_chat && let Some(frame)=self.chat_pending.pop(){
                        return Ok((frame,true));
                    }
                    if !awaiting_authority && let Some(frame) = self.ingress.pop() {
                        return Ok((frame,false));
                    }
                    tokio::time::timeout_at(deadline, self.stream.read()).await
                        .map_err(|_| Error::Timeout(Phase::Play))?
                        .map(|bytes| (ingress::Frame::new(bytes),false)).map_err(Error::from)
                }, if (!awaiting_authority || self.ingress.can_read()) && (!preparing_chat || self.chat_pending.can_read()) => {
                    deadline = tokio::time::Instant::now() + Duration::from_secs(120);
                    result?
                },
                material=async {
                    match self.chat_preparation.as_mut(){Some(future)=>future.await,None=>std::future::pending().await}
                }=>{
                    self.chat_preparation=None;
                    match material {
                        Ok(material)=>{
                            self.chat_announcement=self.chat.install(material,auth::chat::now_millis_protocol()?)?;
                            tracing::info!(target:"java_auth", signing_ready=self.chat.signing_ready(),"Java chat trust preparation completed");
                        }
                        Err(error)=>tracing::warn!(target:"java_auth",%error,"Java chat trust preparation unavailable; authenticated game connection retained"),
                    }
                    continue;
                },
                command = async {
                    match &mut self.commands {
                        Some(commands) => commands.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match command {
                        Some(command) => {
                            if self.command(command).await? {
                                deadline = tokio::time::Instant::now() + Duration::from_secs(120);
                            }
                        }
                        None if self.pending_teleport.is_some() || self.pending_rotation.is_some() || self.pending_ping.is_some() => return Err(Error::ConsumerClosed),
                        None => self.commands = None,
                    }
                    continue;
                }
            };
            let mut r = Reader(&packet.bytes);
            let id = r.var()?;
            let name = usize::try_from(id)
                .ok()
                .and_then(|n| target::game::clientbound::NAMES.get(n))
                .ok_or(ProtocolError::Invalid("unknown play packet"))?;
            // Keep-alives are network-thread work in the native client. Game-thread pings
            // and all world packets stay behind the correction until its owner replies.
            if awaiting_authority
                && !matches!(
                    *name,
                    "ClientboundKeepAlivePacket" | "ClientboundDisconnectPacket"
                )
            {
                let mut packet = packet;
                packet.boundary = *name == "ClientboundStartConfigurationPacket";
                self.ingress.push(packet);
                continue;
            }
            let packet_error = |source| Error::PlayPacket {
                packet: *name,
                source,
            };
            if self.common(name, &mut r, packet.keepalive_replied).await? {
                r.finish().map_err(packet_error)?;
                continue;
            }
            if !chat_replay
                && preparing_chat
                && matches!(
                    *name,
                    "ClientboundPlayerChatPacket" | "ClientboundDeleteChatPacket"
                )
            {
                self.chat_pending.push(packet);
                continue;
            }
            if matches!(
                *name,
                "ClientboundPlayerChatPacket"
                    | "ClientboundDisguisedChatPacket"
                    | "ClientboundDeleteChatPacket"
            ) {
                let message = self
                    .chat
                    .receive(name, &mut r, &self.configuration)
                    .map_err(packet_error)?;
                r.finish().map_err(packet_error)?;
                if let Some(message) = message {
                    self.emit(Event::SystemChat(message)).await?;
                }
                if let Some(bytes) = self.chat.acknowledgement() {
                    self.send("ServerboundChatAckPacket", &bytes).await?;
                }
                continue;
            }
            if let Some(message) = chat::decode(name, &mut r).map_err(packet_error)? {
                r.finish().map_err(packet_error)?;
                self.emit(Event::SystemChat(message)).await?;
                continue;
            }
            if *name == "ClientboundBlockChangedAckPacket" {
                let sequence = r.var()?;
                r.finish().map_err(packet_error)?;
                if !self.bootstrapped || sequence < 0 {
                    return Err(ProtocolError::Invalid(
                        "Java block acknowledgement state/sequence",
                    )
                    .into());
                }
                self.emit(Event::BlockAck {
                    dimension_epoch: self.dimension_epoch,
                    sequence,
                })
                .await?;
                continue;
            }
            if let Some(packet) = super::scoreboard::decode(name, &mut r).map_err(packet_error)? {
                r.finish().map_err(packet_error)?;
                if !self.bootstrapped {
                    return Err(ProtocolError::Invalid("scoreboard before world login").into());
                }
                self.emit(Event::Scoreboard(packet)).await?;
                continue;
            }
            if let Some(roster_update) = roster::decode(name, &mut r).map_err(packet_error)? {
                r.finish().map_err(packet_error)?;
                if !self.bootstrapped {
                    return Err(
                        ProtocolError::Invalid("player list before Java world login").into(),
                    );
                }
                if chat_replay {
                    self.chat.observe_roster(&roster_update);
                    continue;
                }
                // Rendering roster admission stays immediate. Security replays the SAME
                // packet order with queued chat once trust discovery completes.
                if preparing_chat {
                    self.chat_pending.push(packet);
                } else {
                    self.chat.observe_roster(&roster_update);
                }
                self.emit(Event::Roster(roster_update)).await?;
                continue;
            }

            if let Some(packet) = inventory::decode(name, &mut r).map_err(packet_error)? {
                r.finish().map_err(packet_error)?;
                if !self.bootstrapped {
                    return Err(ProtocolError::Invalid("inventory before Java world login").into());
                }
                let incarnation = self
                    .menu_lifetime
                    .observe(&packet)
                    .map_err(ProtocolError::Invalid)?;
                self.emit(Event::Inventory {
                    dimension_epoch: self.dimension_epoch,
                    incarnation,
                    packet,
                })
                .await?;
                continue;
            }
            if let Some(packet) = hud::decode(name, &mut r).map_err(packet_error)? {
                r.finish().map_err(packet_error)?;
                if !self.bootstrapped {
                    return Err(ProtocolError::Invalid("status before Java world login").into());
                }
                self.emit(Event::Hud(packet)).await?;
                continue;
            }
            if let Some(mut packet) = entities::decode(name, &mut r).map_err(packet_error)? {
                r.finish().map_err(packet_error)?;
                if !self.bootstrapped {
                    return Err(ProtocolError::Invalid("entity before Java world login").into());
                }
                self.entity_lifetimes
                    .observe(&mut packet)
                    .map_err(ProtocolError::Invalid)?;
                self.emit(Event::Entities {
                    dimension_epoch: self.dimension_epoch,
                    packet,
                })
                .await?;
                continue;
            }
            if let Some(packet) = dynamics::decode(name, &mut r).map_err(packet_error)? {
                r.finish().map_err(packet_error)?;
                if !self.bootstrapped {
                    return Err(ProtocolError::Invalid("player dynamics before world login").into());
                }
                if let dynamics::Packet::Rotation {
                    yaw,
                    relative_yaw,
                    pitch,
                    relative_pitch,
                } = packet
                {
                    if self.commands.is_none() {
                        return Err(Error::ConsumerClosed);
                    }
                    self.correction_sequence = self
                        .correction_sequence
                        .checked_add(1)
                        .ok_or(ProtocolError::Invalid("Java correction sequence overflow"))?;
                    self.pending_rotation = Some(self.correction_sequence);
                    self.emit(Event::Rotation {
                        dimension_epoch: self.dimension_epoch,
                        sequence: self.correction_sequence,
                        yaw,
                        relative_yaw,
                        pitch,
                        relative_pitch,
                    })
                    .await?;
                } else {
                    self.emit(Event::Dynamics {
                        dimension_epoch: self.dimension_epoch,
                        packet,
                    })
                    .await?;
                }
                continue;
            }
            if let Some(context) = self.terrain
                && let Some(packet) = context.decode(name, &mut r).map_err(packet_error)?
            {
                r.finish().map_err(packet_error)?;
                self.emit(Event::Terrain {
                    dimension_epoch: self.dimension_epoch,
                    packet: Box::new(packet),
                })
                .await?;
                continue;
            }
            match *name {
                "ClientboundDelimiterPacket" => {}
                "ClientboundLoginPacket" => {
                    let world = facts::World::read(&mut r).map_err(packet_error)?;
                    r.finish().map_err(packet_error)?;
                    if !self.bootstrapped {
                        self.prepare_terrain()?;
                    }
                    world
                        .validate_identity(self.dimensions.len())
                        .map_err(packet_error)?;
                    self.terrain = Some(self.terrain_context(&world.spawn)?);
                    self.dimension_epoch = self
                        .dimension_epoch
                        .checked_add(1)
                        .ok_or(ProtocolError::Invalid("Java dimension epoch overflow"))?;
                    let old_online = self.chat.online();
                    self.chat_announcement = self.chat.world_login(
                        world.online_mode,
                        world.secure_chat,
                        auth::chat::now_millis_protocol()?,
                    )?;
                    self.chat_pending = ingress::Ingress::new(crate::stream::MAX_UNCOMPRESSED);
                    if !world.online_mode {
                        self.chat_preparation = None;
                    }
                    if world.online_mode && !old_online {
                        self.chat_retry_after = 0;
                    }
                    self.dead = false;
                    self.loaded_epoch = None;
                    self.cancelled_use.clear();
                    self.pending_teleport = None;
                    self.pending_rotation = None;
                    self.menu_lifetime.retire();
                    self.entity_lifetimes.retire();
                    if self.bootstrapped {
                        // Proxies may send another game login without a configuration phase.
                        // Reuse configured registries; do not clone their NBT for every handoff.
                        self.emit(Event::WorldLogin(Box::new(world))).await?;
                    } else {
                        let bootstrap = Bootstrap {
                            generation: self.generation,
                            profile: self.profile.clone(),
                            server_session: self.server_session,
                            // Retained immutable protocol snapshot also supports registry-free reconfiguration.
                            configuration: self.configuration.clone(),
                            world,
                        };
                        bootstrap.validate()?;
                        self.emit(Event::Bootstrap(Box::new(bootstrap))).await?;
                        self.bootstrapped = true;
                    }
                    continue;
                }
                "ClientboundPlayerPositionPacket" => {
                    if !self.bootstrapped {
                        return Err(ProtocolError::Invalid("position before world login").into());
                    }
                    let position = facts::Teleport::read(&mut r)?;
                    r.finish().map_err(packet_error)?;
                    if self.commands.is_none() {
                        return Err(Error::ConsumerClosed);
                    }
                    self.correction_sequence = self
                        .correction_sequence
                        .checked_add(1)
                        .ok_or(ProtocolError::Invalid("Java correction sequence overflow"))?;
                    self.pending_teleport = Some((position.id, self.correction_sequence));
                    // Admission remains ordered; ingress still services transport keep-alives.
                    self.emit(Event::Teleport {
                        dimension_epoch: self.dimension_epoch,
                        sequence: self.correction_sequence,
                        correction: position,
                    })
                    .await?;
                    continue;
                }
                "ClientboundChunkBatchFinishedPacket" => {
                    let size = r.var()?;
                    if size < 0 {
                        return Err(ProtocolError::Invalid("chunk batch size").into());
                    }
                    self.send("ServerboundChunkBatchReceivedPacket", &2f32.to_be_bytes())
                        .await?;
                }
                "ClientboundChunkBatchStartPacket" => {}
                "ClientboundSetHealthPacket" => {
                    let vitals = facts::Vitals::read(&mut r)?;
                    self.dead = vitals.health <= 0.;
                    self.emit(Event::Vitals(vitals)).await?;
                }
                "ClientboundPlayerCombatKillPacket" => {
                    let player_id = r.var()?;
                    let message = crate::nbt::read(&mut r)?;
                    self.dead = true;
                    self.menu_lifetime.retire();
                    self.entity_lifetimes.retire();
                    self.emit(Event::Death { player_id, message }).await?;
                }
                "ClientboundRespawnPacket" => {
                    self.dead = false;
                    let spawn = facts::Spawn::read(&mut r)?;
                    let keep = r.byte()?;
                    if keep & !3 != 0 {
                        return Err(ProtocolError::Invalid("Java respawn keep flags").into());
                    }
                    self.terrain = Some(self.terrain_context(&spawn)?);
                    self.dimension_epoch = self
                        .dimension_epoch
                        .checked_add(1)
                        .ok_or(ProtocolError::Invalid("Java dimension epoch overflow"))?;
                    // This is the server's new player/world, not proof from a sent respawn request.
                    self.loaded_epoch = None;
                    self.cancelled_use.clear();
                    self.pending_teleport = None;
                    self.pending_rotation = None;
                    self.menu_lifetime.retire();
                    self.entity_lifetimes.retire();
                    self.emit(Event::Respawn {
                        spawn,
                        keep_attribute_modifiers: keep & 1 != 0,
                        keep_metadata: keep & 2 != 0,
                    })
                    .await?;
                }
                "ClientboundStartConfigurationPacket" => {
                    r.finish().map_err(packet_error)?;
                    self.reconfigure().await?;
                    deadline = tokio::time::Instant::now() + Duration::from_secs(120);
                    continue;
                }
                "ClientboundShowDialogGamePacket" => {
                    return Err(Error::Unsupported(
                        "server dialogs requiring user interaction",
                    ));
                }
                // The target table proves identity, not field validity. These bytes are deliberately
                // outside admitted world state until the owning gameplay decoder is implemented.
                name if optional_observation(name) => {
                    self.emit(Event::DeferredPlay {
                        name: name.to_owned(),
                        bytes: r.0.to_vec(),
                    })
                    .await?;
                    continue;
                }
                _ => return Err(Error::Unsupported(name)),
            }
            r.finish().map_err(packet_error)?;
        }
    }
}

/// Passive notifications in the pinned codec require no transport response. They remain
/// unvalidated observations, not admitted gameplay state. Interactive/unknown packets fail.
const OPTIONAL_OBSERVATIONS: &[&str] = &[
    "ClientboundAnimatePacket",
    "ClientboundAwardStatsPacket",
    "ClientboundBlockDestructionPacket",
    "ClientboundBossEventPacket",
    "ClientboundChangeDifficultyPacket",
    "ClientboundClearTitlesPacket",
    "ClientboundCommandSuggestionsPacket",
    "ClientboundCommandsPacket",
    "ClientboundCustomChatCompletionsPacket",
    "ClientboundDamageEventPacket",
    "ClientboundDebugBlockValuePacket",
    "ClientboundDebugChunkValuePacket",
    "ClientboundDebugEntityValuePacket",
    "ClientboundDebugEventPacket",
    "ClientboundDebugSamplePacket",
    "ClientboundEntityEventPacket",
    "ClientboundExplodePacket",
    "ClientboundAddTransientBlockPacket",
    "ClientboundGameRuleValuesPacket",
    "ClientboundGameTestHighlightPosPacket",
    "ClientboundHurtAnimationPacket",
    "ClientboundInitializeBorderPacket",
    "ClientboundLevelEventPacket",
    "ClientboundLevelParticlesPacket",
    "ClientboundLowDiskSpaceWarningPacket",
    "ClientboundMapItemDataPacket",
    "ClientboundMoveMinecartPacket",
    "ClientboundPlayerCombatEndPacket",
    "ClientboundPlayerCombatEnterPacket",
    "ClientboundPlayerLookAtPacket",
    "ClientboundRecipeBookAddPacket",
    "ClientboundRecipeBookRemovePacket",
    "ClientboundRecipeBookSettingsPacket",
    "ClientboundRemoveMobEffectPacket",
    "ClientboundSelectAdvancementsTabPacket",
    "ClientboundServerDataPacket",
    "ClientboundSetBorderCenterPacket",
    "ClientboundSetBorderLerpSizePacket",
    "ClientboundSetBorderSizePacket",
    "ClientboundSetBorderWarningDelayPacket",
    "ClientboundSetBorderWarningDistancePacket",
    "ClientboundSetCameraPacket",
    "ClientboundSetDefaultSpawnPositionPacket",
    "ClientboundSetEntityLinkPacket",
    "ClientboundSetEquipmentPacket",
    "ClientboundSetPlayerTeamPacket",
    "ClientboundSetSimulationDistancePacket",
    "ClientboundSetSubtitleTextPacket",
    "ClientboundSetTimePacket",
    "ClientboundSetTitleTextPacket",
    "ClientboundSetTitlesAnimationPacket",
    "ClientboundSoundEntityPacket",
    "ClientboundSoundPacket",
    "ClientboundStopSoundPacket",
    "ClientboundSwingAnimationPacket",
    "ClientboundTabListPacket",
    "ClientboundTagQueryPacket",
    "ClientboundTakeItemEntityPacket",
    "ClientboundTickingStatePacket",
    "ClientboundTickingStepPacket",
    "ClientboundUpdateAdvancementsPacket",
    "ClientboundUpdateMobEffectPacket",
    "ClientboundUpdateRecipesPacket",
    "ClientboundProjectilePowerPacket",
    "ClientboundTrackedWaypointPacket",
    "ClientboundClearDialogPacket",
];
fn optional_observation(name: &str) -> bool {
    OPTIONAL_OBSERVATIONS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_policy_only_names_pinned_packets() {
        for name in OPTIONAL_OBSERVATIONS {
            assert!(target::game::clientbound::NAMES.contains(name), "{name}");
        }
        assert!(!optional_observation("ClientboundStartConfigurationPacket"));
        assert!(!optional_observation("ClientboundFuturePacket"));
    }
}
