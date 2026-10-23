//! Native Java Edition discovery, authentication and login/configuration/play handoff.
//!
//! Codecs are independent of Bedrock, Bevy and the Go core. Native TCP I/O lives
//! in [`transport`]; callers must not interpret a status response as play support.

#[cfg(not(target_arch = "wasm32"))]
pub mod address;
#[cfg(not(target_arch = "wasm32"))]
pub mod auth;
pub mod codec;
pub mod nbt;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
pub mod status;
#[cfg(not(target_arch = "wasm32"))]
pub mod stream;
pub mod target;
pub mod terrain;
#[cfg(not(target_arch = "wasm32"))]
pub mod transport;
pub mod wire;

/// A malformed or unsupported Java wire value.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("native Java item component {0} is not implemented")]
    UnsupportedItemComponent(&'static str),
    #[error("invalid Java value: {0}")]
    Invalid(&'static str),
    #[error("truncated Java packet")]
    Truncated,
    #[error("Java VarInt exceeds 32 bits")]
    InvalidVarInt,
    #[error("invalid Java packet length")]
    InvalidFrameLength,
    #[error("Java string exceeds its protocol limit")]
    StringTooLong,
    #[error("invalid Java hostname")]
    InvalidHostname,
    #[error("Java string is not UTF-8")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    #[error("expected Java packet {expected}, received {actual}")]
    UnexpectedPacket { expected: i32, actual: i32 },
    #[error("trailing bytes in Java packet")]
    TrailingData,
    #[error("invalid Java status JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("Java pong did not echo the ping payload")]
    PongMismatch,
}
