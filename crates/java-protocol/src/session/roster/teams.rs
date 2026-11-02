//! Java 26.3 team packet codec. Parameters are the current composite codec, not the older layout.
//! Verified by ProbeTeams.java against official ClientboundSetPlayerTeamPacket.STREAM_CODEC.
use crate::{ProtocolError as Error, nbt, wire::Reader};
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Visibility {
    Always,
    Never,
    OtherTeamsHidden,
    OwnTeamHidden,
}
impl Visibility {
    fn native(id: i32) -> Self {
        match id {
            1 => Self::Never,
            2 => Self::OtherTeamsHidden,
            3 => Self::OwnTeamHidden,
            _ => Self::Always,
        }
    }
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Parameters {
    pub display: nbt::Value,
    pub prefix: nbt::Value,
    pub suffix: nbt::Value,
    pub visibility: Visibility,
    pub collision: i32,
    /// Native TeamColor's base-sixteen formatting colour, independent of display-name styling.
    pub color: Option<char>,
    pub options: u8,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Packet {
    pub team: String,
    pub method: u8,
    pub parameters: Option<Parameters>,
    pub members: Vec<String>,
}
pub fn decode(r: &mut Reader<'_>) -> Result<Packet, Error> {
    let team = r.string(32767)?;
    let method = r.byte()?;
    let parameters = if matches!(method, 0 | 2) {
        let display = nbt::read(r)?;
        let prefix = nbt::read(r)?;
        let suffix = nbt::read(r)?;
        let visibility = Visibility::native(r.var()?);
        let collision = r.var()?;
        let collision = if (0..4).contains(&collision) {
            collision
        } else {
            0
        };
        let color = if r.bool()? {
            let id = r.var()?;
            Some(
                char::from_digit(if (0..16).contains(&id) { id as u32 } else { 0 }, 16)
                    .expect("native base-sixteen colour"),
            )
        } else {
            None
        };
        Some(Parameters {
            display,
            prefix,
            suffix,
            visibility,
            collision,
            color,
            options: r.byte()?,
        })
    } else {
        None
    };
    let members = if matches!(method, 0 | 3 | 4) {
        let count = r.count(r.0.len())?;
        (0..count)
            .map(|_| r.string(32767))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    Ok(Packet {
        team,
        method,
        parameters,
        members,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn native() -> serde_json::Value {
        serde_json::from_str(include_str!("team-native-26.3.json")).unwrap()
    }
    fn bytes(name: &str) -> Vec<u8> {
        let n = native();
        let s = n[name].as_str().unwrap();
        s.as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn current_composite_order_matches_independent_native_packets() {
        let n = native();
        assert_eq!(n["version"], crate::target::GAME_VERSION);
        for (name, method) in [
            ("create", 0),
            ("remove", 1),
            ("update", 2),
            ("join", 3),
            ("leave", 4),
        ] {
            let b = bytes(name);
            let mut r = Reader(&b);
            let packet = decode(&mut r).unwrap();
            r.finish().unwrap();
            assert_eq!(packet.team, "alpha");
            assert_eq!(packet.method, method);
            if matches!(method, 0 | 2) {
                let p = packet.parameters.unwrap();
                assert_eq!(p.prefix, nbt::Value::String("[A] ".into()));
                assert_eq!(p.suffix, nbt::Value::String(" !".into()));
                assert_eq!(p.visibility, Visibility::OtherTeamsHidden);
                assert_eq!(p.collision, 1);
                assert_eq!(p.color, Some('c'));
                assert_eq!(p.options, 2);
            }
            assert_eq!(
                packet.members,
                match method {
                    0 => vec!["Alice"],
                    3 | 4 => vec!["Bob"],
                    _ => vec![],
                }
            );
        }
        assert_eq!(
            n["visibility"],
            serde_json::json!(["always", "never", "hideForOtherTeams", "hideForOwnTeam"])
        );
        assert_eq!(n["colors"].as_array().unwrap().len(), 16);
    }
    #[test]
    fn incomplete_updates_do_not_decode_as_success() {
        let b = bytes("create");
        for end in 0..b.len() {
            assert!(decode(&mut Reader(&b[..end])).is_err(), "prefix {end}");
        }
        let mut trailing = bytes("remove");
        trailing.push(0);
        let mut r = Reader(&trailing);
        decode(&mut r).unwrap();
        assert!(r.finish().is_err());
    }
    #[test]
    fn unknown_well_framed_methods_remain_ignorable_events() {
        let p = decode(&mut Reader(&[1, b'x', 99])).unwrap();
        assert_eq!(p.method, 99);
        assert!(p.parameters.is_none() && p.members.is_empty());
    }
}
