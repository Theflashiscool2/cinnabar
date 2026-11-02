//! Java 26.3 scoreboard packets, checked against the pinned shipped stream codecs.
use crate::{ProtocolError as Error, nbt, wire::Reader};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub enum NumberFormat {
    Blank,
    Styled(nbt::Value),
    Fixed(nbt::Value),
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub enum Packet {
    Objective {
        name: String,
        method: u8,
        display: Option<(nbt::Value, i32, Option<NumberFormat>)>,
    },
    Display {
        slot: i32,
        objective: String,
    },
    Score {
        owner: String,
        objective: String,
        score: i32,
        display: Option<nbt::Value>,
        format: Option<NumberFormat>,
    },
    Reset {
        owner: String,
        objective: Option<String>,
    },
}
fn format(r: &mut Reader<'_>) -> Result<Option<NumberFormat>, Error> {
    if !r.bool()? {
        return Ok(None);
    }
    Ok(Some(match r.var()? {
        0 => NumberFormat::Blank,
        1 => NumberFormat::Styled(nbt::read(r)?),
        2 => NumberFormat::Fixed(nbt::read(r)?),
        _ => return Err(Error::Invalid("unknown Java score number format")),
    }))
}
pub(super) fn decode(name: &str, r: &mut Reader<'_>) -> Result<Option<Packet>, Error> {
    Ok(Some(match name {
        "ClientboundSetObjectivePacket" => {
            let name = r.string(32767)?;
            let method = r.byte()?;
            let display = if matches!(method, 0 | 2) {
                Some((nbt::read(r)?, r.var()?, format(r)?))
            } else {
                None
            };
            Packet::Objective {
                name,
                method,
                display,
            }
        }
        "ClientboundSetDisplayObjectivePacket" => Packet::Display {
            slot: r.var()?,
            objective: r.string(32767)?,
        },
        "ClientboundSetScorePacket" => Packet::Score {
            owner: r.string(32767)?,
            objective: r.string(32767)?,
            score: r.var()?,
            display: if r.bool()? { Some(nbt::read(r)?) } else { None },
            format: format(r)?,
        },
        "ClientboundResetScorePacket" => Packet::Reset {
            owner: r.string(32767)?,
            objective: if r.bool()? {
                Some(r.string(32767)?)
            } else {
                None
            },
        },
        _ => return Ok(None),
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }
    #[test]
    fn shipped_stream_codec_vectors_and_every_truncation() {
        // Independently emitted by official 26.3 STREAM_CODEC, ProbeSidebar.java.
        for (name, hex) in [
            (
                "ClientboundSetObjectivePacket",
                "056c6f626279000800054c6f6262790000",
            ),
            ("ClientboundSetObjectivePacket", "056c6f62627901"),
            ("ClientboundSetDisplayObjectivePacket", "01056c6f626279"),
            ("ClientboundSetDisplayObjectivePacket", "0100"),
            (
                "ClientboundSetScorePacket",
                "05416c696365056c6f626279070000",
            ),
            (
                "ClientboundSetScorePacket",
                "05416c696365056c6f626279070108000457696e730100",
            ),
            (
                "ClientboundSetScorePacket",
                "05416c696365056c6f6262790700010208000158",
            ),
            (
                "ClientboundSetScorePacket",
                "05416c696365056c6f626279070001010a080005636f6c6f720004676f6c6400",
            ),
            (
                "ClientboundSetObjectivePacket",
                "056c6f626279020800075570646174656400010208000121",
            ),
            ("ClientboundResetScorePacket", "05416c69636501056c6f626279"),
            ("ClientboundResetScorePacket", "05416c69636500"),
        ] {
            let b = bytes(hex);
            let mut r = Reader(&b);
            assert!(decode(name, &mut r).unwrap().is_some());
            r.finish().unwrap();
            for end in 0..b.len() {
                assert!(
                    decode(name, &mut Reader(&b[..end])).is_err(),
                    "{name} {end}"
                );
            }
        }
        let b = bytes("05416c696365056c6f626279070108000457696e730100");
        assert!(
            matches!(decode("ClientboundSetScorePacket",&mut Reader(&b)).unwrap(),Some(Packet::Score {score:7,display:Some(nbt::Value::String(s)),format:Some(NumberFormat::Blank),..}) if s=="Wins")
        );
    }
}
