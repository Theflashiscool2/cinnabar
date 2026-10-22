//! Ordered teleport admission. The socket owner serializes the domain owner's resolved pose.
use super::*;

impl Connection {
    /// Returns true only when the currently outstanding correction is acknowledged on the wire.
    pub(super) async fn command(&mut self, command: Command) -> Result<bool, Error> {
        match command {
            Command::AcceptRotation {
                dimension_epoch,
                sequence,
                yaw,
                pitch,
            } if dimension_epoch == self.dimension_epoch
                && self.pending_rotation == Some(sequence) =>
            {
                if !yaw.is_finite() || !pitch.is_finite() {
                    return Err(ProtocolError::Invalid("nonfinite admitted Java rotation").into());
                }
                let mut payload = Vec::with_capacity(9);
                payload.extend(yaw.to_be_bytes());
                payload.extend(pitch.to_be_bytes());
                payload.push(0); // Vanilla's immediate correction reply has both ground flags false.
                self.send("ServerboundMovePlayerRotPacket", &payload)
                    .await?;
                self.pending_rotation = None;
                return Ok(true);
            }
            Command::AcceptRotation { .. } => {}
            Command::AcceptTeleport {
                dimension_epoch,
                sequence,
                id,
                position,
                yaw,
                pitch,
            } if dimension_epoch == self.dimension_epoch
                && self.pending_teleport == Some((id, sequence)) =>
            {
                let payload = acknowledgement(id, position, yaw, pitch)?;
                self.send("ServerboundAcceptTeleportationPacket", &payload)
                    .await?;
                self.pending_teleport = None;
                return Ok(true);
            }
            Command::AcceptTeleport { .. } => {}
            Command::Respawn if self.dead => {
                self.send("ServerboundClientCommandPacket", &[0]).await?
            }
            Command::Respawn => {}
            Command::PlayerLoaded { dimension_epoch }
                if dimension_epoch == self.dimension_epoch
                    && self.bootstrapped
                    && self.pending_teleport.is_none()
                    && self.pending_rotation.is_none()
                    && self.loaded_epoch != Some(dimension_epoch) =>
            {
                self.send("ServerboundPlayerLoadedPacket", &[]).await?;
                self.loaded_epoch = Some(dimension_epoch);
            }
            Command::PlayerLoaded { .. } => {}
        }
        Ok(false)
    }
}

fn acknowledgement(
    id: i32,
    position: [f64; 3],
    yaw: f32,
    pitch: f32,
) -> Result<Vec<u8>, ProtocolError> {
    if position.iter().any(|v| !v.is_finite()) || !yaw.is_finite() || !pitch.is_finite() {
        return Err(ProtocolError::Invalid("nonfinite admitted Java teleport"));
    }
    let mut bytes = Vec::with_capacity(37);
    codec::write_varint(id, &mut bytes);
    for coordinate in position {
        bytes.extend(coordinate.to_be_bytes());
    }
    bytes.extend(yaw.to_be_bytes());
    bytes.extend(pitch.to_be_bytes());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acknowledgement_contains_resolved_double_coordinates_and_float_rotation() {
        // Independent IEEE-754/VarInt vector: id 128; x 1, y -64, z .5; yaw 90, pitch -45.
        assert_eq!(
            acknowledgement(128, [1., -64., 0.5], 90., -45.).unwrap(),
            [
                0x80, 0x01, 0x3f, 0xf0, 0, 0, 0, 0, 0, 0, 0xc0, 0x50, 0, 0, 0, 0, 0, 0, 0x3f, 0xe0,
                0, 0, 0, 0, 0, 0, 0x42, 0xb4, 0, 0, 0xc2, 0x34, 0, 0,
            ]
        );
        assert!(acknowledgement(0, [f64::INFINITY, 0., 0.], 0., 0.).is_err());
    }
}
