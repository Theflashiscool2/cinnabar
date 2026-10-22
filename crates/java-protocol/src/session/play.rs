//! Essential play bootstrap and acknowledgements. Deferred packets are never gameplay facts.
use super::*;
impl Connection {
    pub(super) async fn play(&mut self) -> Result<(), Error> {
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(120), self.stream.read())
                .await
                .map_err(|_| Error::Timeout(Phase::Play))??;
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
            match *name {
                "ClientboundDelimiterPacket" => {}
                "ClientboundLoginPacket" => {
                    if self.bootstrapped {
                        return Err(ProtocolError::Invalid("duplicate play login").into());
                    }
                    let world = facts::World::read(&mut r)?;
                    r.finish()?;
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
                    let mut reply = Vec::new();
                    if position.relative != 0 {
                        return Err(Error::Unsupported(
                            "relative teleports before movement integration",
                        ));
                    }
                    codec::write_varint(position.id, &mut reply);
                    for coordinate in position.position {
                        reply.extend(coordinate.to_be_bytes());
                    }
                    reply.extend(position.yaw.to_be_bytes());
                    reply.extend(position.pitch.to_be_bytes());
                    self.send("ServerboundAcceptTeleportationPacket", &reply)
                        .await?;
                    self.emit(Event::Teleport(position)).await?;
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
                "ClientboundRespawnPacket" => {
                    let spawn = facts::Spawn::read(&mut r)?;
                    r.byte()?;
                    self.emit(Event::Respawn(spawn)).await?;
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
                "ClientboundUpdateTagsPacket" => {
                    return Err(Error::Unsupported("mid-session tag replacement"));
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
    "ClientboundAddEntityPacket",
    "ClientboundAnimatePacket",
    "ClientboundAwardStatsPacket",
    "ClientboundBlockChangedAckPacket",
    "ClientboundBlockDestructionPacket",
    "ClientboundBlockEntityDataPacket",
    "ClientboundBlockEventPacket",
    "ClientboundBlockUpdatePacket",
    "ClientboundBossEventPacket",
    "ClientboundChangeDifficultyPacket",
    "ClientboundChunksBiomesPacket",
    "ClientboundClearTitlesPacket",
    "ClientboundCommandSuggestionsPacket",
    "ClientboundCommandsPacket",
    "ClientboundContainerClosePacket",
    "ClientboundContainerSetContentPacket",
    "ClientboundContainerSetDataPacket",
    "ClientboundContainerSetSlotPacket",
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
    "ClientboundEntityPositionSyncPacket",
    "ClientboundExplodePacket",
    "ClientboundAddTransientBlockPacket",
    "ClientboundForgetLevelChunkPacket",
    "ClientboundGameEventPacket",
    "ClientboundGameRuleValuesPacket",
    "ClientboundGameTestHighlightPosPacket",
    "ClientboundHurtAnimationPacket",
    "ClientboundInitializeBorderPacket",
    "ClientboundLevelChunkWithLightPacket",
    "ClientboundLevelEventPacket",
    "ClientboundLevelParticlesPacket",
    "ClientboundLightUpdatePacket",
    "ClientboundLowDiskSpaceWarningPacket",
    "ClientboundMapItemDataPacket",
    "ClientboundMoveEntityPosPacket",
    "ClientboundMoveEntityPosRotPacket",
    "ClientboundMoveMinecartPacket",
    "ClientboundMoveEntityRotPacket",
    "ClientboundPlayerAbilitiesPacket",
    "ClientboundPlayerCombatEndPacket",
    "ClientboundPlayerCombatEnterPacket",
    "ClientboundPlayerInfoRemovePacket",
    "ClientboundPlayerInfoUpdatePacket",
    "ClientboundPlayerLookAtPacket",
    "ClientboundPlayerRotationPacket",
    "ClientboundRecipeBookAddPacket",
    "ClientboundRecipeBookRemovePacket",
    "ClientboundRecipeBookSettingsPacket",
    "ClientboundRemoveEntitiesPacket",
    "ClientboundRemoveMobEffectPacket",
    "ClientboundResetScorePacket",
    "ClientboundRotateHeadPacket",
    "ClientboundSectionBlocksUpdatePacket",
    "ClientboundSelectAdvancementsTabPacket",
    "ClientboundServerDataPacket",
    "ClientboundSetActionBarTextPacket",
    "ClientboundSetBorderCenterPacket",
    "ClientboundSetBorderLerpSizePacket",
    "ClientboundSetBorderSizePacket",
    "ClientboundSetBorderWarningDelayPacket",
    "ClientboundSetBorderWarningDistancePacket",
    "ClientboundSetCameraPacket",
    "ClientboundSetChunkCacheCenterPacket",
    "ClientboundSetChunkCacheRadiusPacket",
    "ClientboundSetCursorItemPacket",
    "ClientboundSetDefaultSpawnPositionPacket",
    "ClientboundSetDisplayObjectivePacket",
    "ClientboundSetEntityDataPacket",
    "ClientboundSetEntityLinkPacket",
    "ClientboundSetEntityMotionPacket",
    "ClientboundSetEquipmentPacket",
    "ClientboundSetExperiencePacket",
    "ClientboundSetHealthPacket",
    "ClientboundSetHeldSlotPacket",
    "ClientboundSetObjectivePacket",
    "ClientboundSetPassengersPacket",
    "ClientboundSetPlayerInventoryPacket",
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
    "ClientboundTeleportEntityPacket",
    "ClientboundTickingStatePacket",
    "ClientboundTickingStepPacket",
    "ClientboundUpdateAdvancementsPacket",
    "ClientboundUpdateAttributesPacket",
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
