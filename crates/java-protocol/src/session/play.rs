//! Essential play bootstrap and acknowledgements. Deferred packets are never gameplay facts.
use super::*;
impl Connection {
    pub(super) async fn play(&mut self) -> Result<(), Error> {
        let mut deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            let packet = tokio::select! {
                result = tokio::time::timeout_at(deadline, self.stream.read()), if self.pending_teleport.is_none() && self.pending_rotation.is_none() => {
                    deadline = tokio::time::Instant::now() + Duration::from_secs(120);
                    result.map_err(|_| Error::Timeout(Phase::Play))??
                },
                _ = tokio::time::sleep_until(deadline), if self.pending_teleport.is_some() || self.pending_rotation.is_some() => {
                    return Err(Error::TeleportAdmissionTimeout);
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
                        None if self.pending_teleport.is_some() || self.pending_rotation.is_some() => return Err(Error::ConsumerClosed),
                        None => self.commands = None,
                    }
                    continue;
                }
            };
            let mut r = Reader(&packet);
            let id = r.var()?;
            let name = usize::try_from(id)
                .ok()
                .and_then(|n| target::game::clientbound::NAMES.get(n))
                .ok_or(ProtocolError::Invalid("unknown play packet"))?;
            if self.common(name, &mut r).await? {
                r.finish()?;
                continue;
            }
            if *name == "ClientboundBlockChangedAckPacket" {
                let sequence = r.var()?;
                r.finish()?;
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
            if let Some(packet) = roster::decode(name, &mut r)? {
                r.finish()?;
                if !self.bootstrapped {
                    return Err(
                        ProtocolError::Invalid("player list before Java world login").into(),
                    );
                }
                self.emit(Event::Roster(packet)).await?;
                continue;
            }
            if let Some(packet) = inventory::decode(name, &mut r)? {
                r.finish()?;
                if !self.bootstrapped {
                    return Err(ProtocolError::Invalid("inventory before Java world login").into());
                }
                self.emit(Event::Inventory {
                    dimension_epoch: self.dimension_epoch,
                    packet,
                })
                .await?;
                continue;
            }
            if let Some(packet) = hud::decode(name, &mut r)? {
                r.finish()?;
                if !self.bootstrapped {
                    return Err(ProtocolError::Invalid("status before Java world login").into());
                }
                self.emit(Event::Hud(packet)).await?;
                continue;
            }
            if let Some(packet) = entities::decode(name, &mut r)? {
                r.finish()?;
                if !self.bootstrapped {
                    return Err(ProtocolError::Invalid("entity before Java world login").into());
                }
                self.emit(Event::Entities {
                    dimension_epoch: self.dimension_epoch,
                    packet,
                })
                .await?;
                continue;
            }
            if let Some(packet) = dynamics::decode(name, &mut r)? {
                r.finish()?;
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
                    deadline = tokio::time::Instant::now() + Duration::from_secs(10);
                    tokio::time::timeout_at(
                        deadline,
                        self.emit(Event::Rotation {
                            dimension_epoch: self.dimension_epoch,
                            sequence: self.correction_sequence,
                            yaw,
                            relative_yaw,
                            pitch,
                            relative_pitch,
                        }),
                    )
                    .await
                    .map_err(|_| Error::TeleportAdmissionTimeout)??;
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
                && let Some(packet) = context.decode(name, &mut r)?
            {
                r.finish()?;
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
                    if self.bootstrapped {
                        return Err(ProtocolError::Invalid("duplicate play login").into());
                    }
                    let world = facts::World::read(&mut r)?;
                    r.finish()?;
                    self.prepare_terrain()?;
                    self.terrain = Some(self.terrain_context(&world.spawn)?);
                    self.dimension_epoch += 1;
                    let bootstrap = Bootstrap {
                        generation: self.generation,
                        profile: self.profile.clone(),
                        server_session: self.server_session,
                        configuration: std::mem::take(&mut self.configuration),
                        world,
                    };
                    bootstrap.validate()?;
                    self.emit(Event::Bootstrap(Box::new(bootstrap))).await?;
                    self.bootstrapped = true;
                    continue;
                }
                "ClientboundPlayerPositionPacket" => {
                    if !self.bootstrapped {
                        return Err(ProtocolError::Invalid("position before world login").into());
                    }
                    let position = facts::Teleport::read(&mut r)?;
                    r.finish()?;
                    if self.commands.is_none() {
                        return Err(Error::ConsumerClosed);
                    }
                    self.correction_sequence = self
                        .correction_sequence
                        .checked_add(1)
                        .ok_or(ProtocolError::Invalid("Java correction sequence overflow"))?;
                    self.pending_teleport = Some((position.id, self.correction_sequence));
                    // Backpressure preserves correction order: no subsequent play packet is
                    // admitted until this correction reaches the player owner and its reply.
                    deadline = tokio::time::Instant::now() + Duration::from_secs(10);
                    tokio::time::timeout_at(
                        deadline,
                        self.emit(Event::Teleport {
                            dimension_epoch: self.dimension_epoch,
                            sequence: self.correction_sequence,
                            correction: position,
                        }),
                    )
                    .await
                    .map_err(|_| Error::TeleportAdmissionTimeout)??;
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
                    self.dimension_epoch += 1;
                    self.emit(Event::Respawn {
                        spawn,
                        keep_attribute_modifiers: keep & 1 != 0,
                        keep_metadata: keep & 2 != 0,
                    })
                    .await?;
                }
                "ClientboundStartConfigurationPacket" => {
                    return Err(Error::Unsupported(
                        "mid-session reconfiguration; reconnect to obtain a fresh world handoff",
                    ));
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
            r.finish()?;
        }
    }
}

/// Passive notifications in the pinned codec require no transport response. They remain
/// unvalidated observations, not admitted gameplay state. Interactive/unknown packets fail.
const OPTIONAL_OBSERVATIONS: &[&str] = &[
    "ClientboundAnimatePacket",
    "ClientboundAwardStatsPacket",
    "ClientboundBlockDestructionPacket",
    "ClientboundBlockEntityDataPacket",
    "ClientboundBlockEventPacket",
    "ClientboundBossEventPacket",
    "ClientboundChangeDifficultyPacket",
    "ClientboundClearTitlesPacket",
    "ClientboundCommandSuggestionsPacket",
    "ClientboundCommandsPacket",
    "ClientboundCooldownPacket",
    "ClientboundCustomChatCompletionsPacket",
    "ClientboundDamageEventPacket",
    "ClientboundDebugBlockValuePacket",
    "ClientboundDebugChunkValuePacket",
    "ClientboundDebugEntityValuePacket",
    "ClientboundDebugEventPacket",
    "ClientboundDebugSamplePacket",
    "ClientboundDeleteChatPacket",
    "ClientboundDisguisedChatPacket",
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
    "ClientboundResetScorePacket",
    "ClientboundSelectAdvancementsTabPacket",
    "ClientboundServerDataPacket",
    "ClientboundSetActionBarTextPacket",
    "ClientboundSetBorderCenterPacket",
    "ClientboundSetBorderLerpSizePacket",
    "ClientboundSetBorderSizePacket",
    "ClientboundSetBorderWarningDelayPacket",
    "ClientboundSetBorderWarningDistancePacket",
    "ClientboundSetCameraPacket",
    "ClientboundSetDefaultSpawnPositionPacket",
    "ClientboundSetDisplayObjectivePacket",
    "ClientboundSetEntityLinkPacket",
    "ClientboundSetEquipmentPacket",
    "ClientboundSetObjectivePacket",
    "ClientboundSetPlayerTeamPacket",
    "ClientboundSetScorePacket",
    "ClientboundSetSimulationDistancePacket",
    "ClientboundSetSubtitleTextPacket",
    "ClientboundSetTimePacket",
    "ClientboundSetTitleTextPacket",
    "ClientboundSetTitlesAnimationPacket",
    "ClientboundSoundEntityPacket",
    "ClientboundSoundPacket",
    "ClientboundStopSoundPacket",
    "ClientboundSwingAnimationPacket",
    "ClientboundSystemChatPacket",
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
