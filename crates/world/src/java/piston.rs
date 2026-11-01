//! Received moving-piston data. Missing entity data is not a fabricated moving air block.
//! The pinned native loadAdditional defaults apply only to a received compound.
use crate::NbtValue;
use std::collections::BTreeMap;

/// PistonMovingBlockEntity's native loadAdditional default, not unknown-state recovery.
pub const DEFAULT_MOVED_BLOCK: &str = "minecraft:air";

/// The pinned protocol's Direction legacy-id vector, verified by InspectMovingPiston.
pub fn step(facing: u8) -> Option<[i32; 3]> {
    const DIRECTIONS: [[i32; 3]; 6] = [
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
        [-1, 0, 0],
        [1, 0, 0],
    ];
    DIRECTIONS.get(usize::from(facing)).copied()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub name: String,
    pub properties: BTreeMap<String, String>,
    /// Native Direction legacy id: down, up, north, south, west, east.
    pub facing: u8,
    pub progress: f32,
    pub extending: bool,
    pub source: bool,
}

impl Snapshot {
    pub fn read(data: &NbtValue) -> Result<Self, &'static str> {
        let NbtValue::Compound(data) = data else {
            return Err("Java piston compound expected");
        };
        let mut name = DEFAULT_MOVED_BLOCK.to_owned();
        let mut properties = BTreeMap::new();
        if let Some(state) = data.get("blockState") {
            match state {
                NbtValue::String(value) => name = value.to_string(),
                NbtValue::Compound(state) => {
                    let Some(NbtValue::String(value)) = state.get("Name") else {
                        return Err("Java piston blockState Name expected");
                    };
                    name = value.to_string();
                    if let Some(props) = state.get("Properties") {
                        let NbtValue::Compound(props) = props else {
                            return Err("Java piston blockState Properties compound expected");
                        };
                        for (key, value) in props.iter() {
                            let NbtValue::String(value) = value else {
                                return Err("Java piston block property string expected");
                            };
                            properties.insert(key.to_owned(), value.to_string());
                        }
                    }
                }
                _ => return Err("Java piston named blockState expected"),
            }
        }
        // The legacy codec narrows to a byte, then Direction uses abs(id % 6).
        let facing = (byte(data.get("facing"))? % 6).unsigned_abs();
        let progress = number(data.get("progress"), 0.0)? as f32;
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err("invalid Java piston progress");
        }
        Ok(Self {
            name,
            properties,
            facing,
            progress,
            extending: boolean(data.get("extending"))?,
            source: boolean(data.get("source"))?,
        })
    }
}

fn byte(value: Option<&NbtValue>) -> Result<i8, &'static str> {
    Ok(match value {
        None => 0,
        Some(NbtValue::Byte(n)) => *n,
        Some(NbtValue::Short(n)) => *n as i8,
        Some(NbtValue::Int(n)) => *n as i8,
        Some(NbtValue::Long(n)) => *n as i8,
        Some(NbtValue::Float(n)) => *n as i32 as i8,
        Some(NbtValue::Double(n)) => *n as i32 as i8,
        _ => return Err("Java piston numeric byte expected"),
    })
}

// TagValueInput boolean reads use NumericTag.byteValue (floor for floating tags),
// whereas the legacy facing codec goes through Number.byteValue (truncation).
fn boolean(value: Option<&NbtValue>) -> Result<bool, &'static str> {
    Ok(match value {
        Some(NbtValue::Float(n)) => n.floor() as i32 as i8 != 0,
        Some(NbtValue::Double(n)) => n.floor() as i32 as i8 != 0,
        _ => byte(value)? != 0,
    })
}

fn number(value: Option<&NbtValue>, default: f64) -> Result<f64, &'static str> {
    Ok(match value {
        None => default,
        Some(NbtValue::Byte(n)) => f64::from(*n),
        Some(NbtValue::Short(n)) => f64::from(*n),
        Some(NbtValue::Int(n)) => f64::from(*n),
        Some(NbtValue::Long(n)) => *n as f64,
        Some(NbtValue::Float(n)) => f64::from(*n),
        Some(NbtValue::Double(n)) => *n,
        _ => return Err("Java piston numeric field expected"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compound<const N: usize>(pairs: [(&str, NbtValue); N]) -> NbtValue {
        let mut c = crate::NbtCompound::default();
        for (k, v) in pairs {
            c.insert(k, v);
        }
        NbtValue::Compound(c)
    }
    #[test]
    fn native_compound_defaults_and_named_properties_remain_distinct() {
        let empty = Snapshot::read(&compound([])).unwrap();
        assert_eq!(empty.name, "minecraft:air");
        assert_eq!(
            (empty.facing, empty.progress, empty.extending, empty.source),
            (0, 0., false, false)
        );
        let data = compound([
            (
                "blockState",
                compound([
                    ("Name", NbtValue::String("minecraft:oak_stairs".into())),
                    (
                        "Properties",
                        compound([("facing", NbtValue::String("west".into()))]),
                    ),
                ]),
            ),
            ("facing", NbtValue::Int(5)),
            ("progress", NbtValue::Float(0.5)),
            ("extending", NbtValue::Byte(1)),
            ("source", NbtValue::Byte(0)),
        ]);
        let snapshot = Snapshot::read(&data).unwrap();
        assert_eq!(snapshot.name, "minecraft:oak_stairs");
        assert_eq!(snapshot.properties["facing"], "west");
        assert_eq!(
            (
                snapshot.facing,
                snapshot.progress,
                snapshot.extending,
                snapshot.source
            ),
            (5, 0.5, true, false)
        );
    }
    #[test]
    fn malformed_data_does_not_silently_replace_visible_blocks_with_air() {
        assert!(Snapshot::read(&NbtValue::String("bad".into())).is_err());
        for (key, value) in [
            ("blockState", compound([])),
            ("progress", NbtValue::Float(f32::NAN)),
            ("progress", NbtValue::Float(2.)),
            ("source", NbtValue::String("true".into())),
        ] {
            assert!(Snapshot::read(&compound([(key, value)])).is_err());
        }
    }
    #[test]
    fn original_codec_compact_names_and_numeric_conversions_are_preserved() {
        // Independently observed with InspectMovingPiston --snapshots on the pinned client.
        for (value, facing) in [(-1, 1), (6, 0), (256, 0)] {
            assert_eq!(
                Snapshot::read(&compound([("facing", NbtValue::Int(value))]))
                    .unwrap()
                    .facing,
                facing
            );
        }
        let data = Snapshot::read(&compound([
            ("blockState", NbtValue::String("minecraft:stone".into())),
            ("facing", NbtValue::Float(-1.5)),
            ("extending", NbtValue::Float(-0.75)),
            ("source", NbtValue::Double(0.75)),
        ]))
        .unwrap();
        assert_eq!(data.name, "minecraft:stone");
        assert_eq!((data.facing, data.extending, data.source), (1, true, false));
        let wrapped = Snapshot::read(&compound([
            ("extending", NbtValue::Int(256)),
            ("source", NbtValue::Int(256)),
        ]))
        .unwrap();
        assert!(!wrapped.extending && !wrapped.source);
    }
}
