//! Encodes command arguments using the same certificate, message index and ACK window as chat.
use super::State;
use crate::{ProtocolError as Error, codec, wire};
pub(crate) struct Encoded {
    pub packet: &'static str,
    pub payload: Vec<u8>,
    pub warning: Option<&'static str>,
}
impl State {
    pub fn outgoing_command(
        &mut self,
        text: &str,
        timestamp: i64,
        salt: i64,
    ) -> Result<Encoded, Error> {
        let arguments = match self
            .commands
            .as_ref()
            .map(|tree| tree.arguments(text, self.command_permission))
            .transpose()
        {
            Ok(arguments) => arguments.unwrap_or_default(),
            Err(Error::UnsupportedCommandArgument(_)) if !self.secure => {
                return Ok(Encoded {
                    packet: "ServerboundChatCommandPacket",
                    payload: super::command(text)?,
                    warning: Some(
                        "Command sent without chat signatures: this argument parser is not implemented. The server decides whether to accept it.",
                    ),
                });
            }
            Err(error) => return Err(error),
        };
        if arguments.is_empty() {
            return Ok(Encoded {
                packet: "ServerboundChatCommandPacket",
                payload: super::command(text)?,
                warning: None,
            });
        }
        if self.secure && self.local.is_none() {
            return Err(Error::Invalid(
                "signed Java command certificate is unavailable",
            ));
        }
        let mut out = Vec::new();
        wire::string(text, &mut out)?;
        out.extend(timestamp.to_be_bytes());
        out.extend(salt.to_be_bytes());
        // Work on snapshots so no failed parser, signature or field encoding consumes a
        // message index or acknowledgement that a subsequent packet would need.
        let mut window = self.window.clone();
        let (update, last_seen) = window.update_with_signatures();
        let mut local = self.local.clone();
        codec::write_varint(
            if local.is_some() {
                arguments.len() as i32
            } else {
                0
            },
            &mut out,
        );
        if let Some(signer) = &mut local {
            for (name, value) in arguments {
                codec::write_string(&name, 16, &mut out)?;
                out.extend(signer.sign(&value, timestamp, salt, &last_seen)?);
            }
        }
        out.extend(update);
        self.window = window;
        self.local = local;
        Ok(Encoded {
            packet: "ServerboundChatCommandSignedPacket",
            payload: out,
            warning: None,
        })
    }
}
