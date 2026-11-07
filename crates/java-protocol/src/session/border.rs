//! Version-matched border observations; geometry and ownership remain in the world.
use crate::{ProtocolError, wire::Reader};
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub enum Packet {
    Initialize {
        center: [f64; 2],
        old_size: f64,
        new_size: f64,
        millis: i64,
        absolute_max: i32,
        warning_blocks: i32,
        warning_time: i32,
    },
    Center([f64; 2]),
    Size(f64),
    Lerp {
        from: f64,
        to: f64,
        millis: i64,
    },
    WarningBlocks(i32),
    WarningTime(i32),
}
fn number(r: &mut Reader<'_>) -> Result<f64, ProtocolError> {
    // Native codecs accept raw doubles. Geometry preserves the absolute clamp;
    // NaN constraints never become an unrestricted border when sampled.
    Ok(f64::from_bits(r.i64()? as u64))
}
pub(super) fn decode(name: &str, r: &mut Reader<'_>) -> Result<Option<Packet>, ProtocolError> {
    Ok(Some(match name {
        "ClientboundInitializeBorderPacket" => Packet::Initialize {
            center: [number(r)?, number(r)?],
            old_size: number(r)?,
            new_size: number(r)?,
            millis: r.varlong()? as i64,
            absolute_max: r.var()?,
            warning_blocks: r.var()?,
            warning_time: r.var()?,
        },
        "ClientboundSetBorderCenterPacket" => Packet::Center([number(r)?, number(r)?]),
        "ClientboundSetBorderSizePacket" => Packet::Size(number(r)?),
        "ClientboundSetBorderLerpSizePacket" => Packet::Lerp {
            from: number(r)?,
            to: number(r)?,
            millis: r.varlong()? as i64,
        },
        "ClientboundSetBorderWarningDistancePacket" => Packet::WarningBlocks(r.var()?),
        "ClientboundSetBorderWarningDelayPacket" => Packet::WarningTime(r.var()?),
        _ => return Ok(None),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_border_vectors_consume_every_field_and_reject_truncation() {
        // Encoded/decoded by the verified26.3 native STREAM_CODECs. Distinct
        // warning values and a nonzero VarLong catch swapped/missing fields.
        for (name, hex) in [
            (
                "ClientboundInitializeBorderPacket",
                "3ff4000000000000c0040000000000004040000000000000404000000000000000e807070b",
            ),
            (
                "ClientboundSetBorderCenterPacket",
                "3ff4000000000000c004000000000000",
            ),
            ("ClientboundSetBorderSizePacket", "4040000000000000"),
            ("ClientboundSetBorderWarningDistancePacket", "07"),
            ("ClientboundSetBorderWarningDelayPacket", "0b"),
            (
                "ClientboundSetBorderLerpSizePacket",
                "404000000000000040300000000000009423",
            ),
        ] {
            let bytes: Vec<_> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            let mut reader = Reader(&bytes);
            let packet = decode(name, &mut reader).unwrap().unwrap();
            reader.finish().unwrap();
            match packet {
                Packet::Initialize {
                    center,
                    old_size,
                    new_size,
                    millis,
                    absolute_max,
                    warning_blocks,
                    warning_time,
                } => {
                    assert_eq!(center, [1.25, -2.5]);
                    assert_eq!((old_size, new_size), (32., 32.));
                    assert_eq!(
                        (millis, absolute_max, warning_blocks, warning_time),
                        (0, 1000, 7, 11)
                    );
                }
                Packet::Lerp { from, to, millis } => {
                    assert_eq!((from, to, millis), (32., 16., 4500));
                }
                _ => {}
            }
            for end in 0..bytes.len() {
                assert!(decode(name, &mut Reader(&bytes[..end])).is_err());
            }
            let mut trailing = bytes;
            trailing.push(0);
            let mut reader = Reader(&trailing);
            decode(name, &mut reader).unwrap();
            assert!(reader.finish().is_err());
        }
    }
}
