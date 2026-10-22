//! Java handshake and status-state packets, without transport or clock access.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ProtocolError, codec};

pub const DEFAULT_PORT: u16 = 25565;
/// Discovery sentinel; a successful query does not imply login compatibility.
pub const DISCOVERY_PROTOCOL: i32 = -1;
pub const MAX_HOST_UNITS: usize = 255;
pub const MAX_STATUS_UNITS: usize = 32767;
const HANDSHAKE_ID: i32 = 0;
const STATUS_ID: i32 = 0;
const PING_ID: i32 = 1;
const STATUS_INTENTION: i32 = 1;

/// A server's advertised version, which has not been validated for gameplay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerVersion {
    pub name: String,
    pub protocol: i32,
}

/// Advertised population. Samples stay as JSON to preserve proxy extensions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerPlayers {
    pub max: i32,
    pub online: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Status fields may be omitted by proxies or privacy-configured servers.
/// Rich-text descriptions and unknown extension fields are preserved, not rendered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<ServerVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub players: Option<ServerPlayers>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Builds a framed handshake selecting status, retaining the original virtual host.
pub fn handshake(host: &str, port: u16, protocol: i32) -> Result<Vec<u8>, ProtocolError> {
    if host.is_empty() || host.contains('\0') {
        return Err(ProtocolError::InvalidHostname);
    }
    let mut body = Vec::new();
    codec::write_varint(HANDSHAKE_ID, &mut body);
    codec::write_varint(protocol, &mut body);
    codec::write_string(host, MAX_HOST_UNITS, &mut body)?;
    body.extend_from_slice(&port.to_be_bytes());
    codec::write_varint(STATUS_INTENTION, &mut body);
    codec::frame(&body)
}

/// Builds the framed empty status request.
pub fn request() -> Vec<u8> {
    let mut body = Vec::new();
    codec::write_varint(STATUS_ID, &mut body);
    codec::frame(&body).expect("one packet ID fits a Java frame")
}

/// Builds a framed ping with a caller-owned echo token, unrelated to wall time.
pub fn ping(token: i64) -> Vec<u8> {
    let mut body = Vec::new();
    codec::write_varint(PING_ID, &mut body);
    body.extend_from_slice(&token.to_be_bytes());
    codec::frame(&body).expect("a ping fits a Java frame")
}

/// Decodes an unframed status response, requiring complete packet consumption.
pub fn decode_response(mut body: &[u8]) -> Result<ServerStatus, ProtocolError> {
    expect_id(&mut body, STATUS_ID)?;
    let json = codec::read_string(&mut body, MAX_STATUS_UNITS)?;
    finish(body)?;
    Ok(serde_json::from_str(json)?)
}

/// Checks the packet ID, exact length and echoed token of an unframed pong.
pub fn check_pong(mut body: &[u8], token: i64) -> Result<(), ProtocolError> {
    expect_id(&mut body, PING_ID)?;
    let (payload, rest) = body.split_at_checked(8).ok_or(ProtocolError::Truncated)?;
    finish(rest)?;
    if payload != token.to_be_bytes() {
        return Err(ProtocolError::PongMismatch);
    }
    Ok(())
}

/// Rejects packets from a different state or exchange.
fn expect_id(body: &mut &[u8], expected: i32) -> Result<(), ProtocolError> {
    let actual = codec::read_varint(body)?;
    if actual != expected {
        return Err(ProtocolError::UnexpectedPacket { expected, actual });
    }
    Ok(())
}

/// Rejects extra fields after a known packet's payload.
fn finish(rest: &[u8]) -> Result<(), ProtocolError> {
    if !rest.is_empty() {
        return Err(ProtocolError::TrailingData);
    }
    Ok(())
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;

/// Extracts display text only. Click/hover actions, URLs and translation programs are never run.
pub fn plain_text(value: &serde_json::Value) -> String {
    fn append(value: &serde_json::Value, out: &mut String, depth: usize) {
        if depth > 32 || out.len() >= 4096 {
            return;
        }
        match value {
            serde_json::Value::String(s) => {
                for c in s.chars().filter(|c| !c.is_control()).take(4096 - out.len()) {
                    out.push(c);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    append(item, out, depth + 1)
                }
            }
            serde_json::Value::Object(map) => {
                if let Some(text) = map.get("text").or_else(|| map.get("translate")) {
                    append(text, out, depth + 1)
                }
                if let Some(extra) = map.get("extra") {
                    append(extra, out, depth + 1)
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    append(value, &mut out, 0);
    out
}
