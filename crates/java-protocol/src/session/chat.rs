//! Version-pinned unsigned commands and system chat. Player chat signatures and
//! last-seen acknowledgements are deliberately not synthesized by this path.
use crate::{
    ProtocolError as Error, nbt,
    wire::{self, Reader},
};

/// FriendlyByteBuf.writeUtf/readUtf default used by ServerboundChatCommandPacket.
pub const COMMAND_MAX_UTF16: usize = 32767;
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
