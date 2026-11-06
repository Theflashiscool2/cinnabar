//! Native metadata serializer framing. Complex unimplemented payloads explicitly block visuals.
use crate::{ProtocolError as Error, nbt, target, wire::Reader};
use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub enum Value {
    Byte(u8),
    Item(Option<super::super::inventory::Stack>),
    Int(i32),
    Long(i64),
    Float(f32),
    String(String),
    Component(Option<nbt::Value>),
    Bool(bool),
    Vector(Vec<f32>),
    Position(Option<i64>),
    Uuid(Option<[u8; 16]>),
    Villager([i32; 3]),
    GlobalPosition(Option<(String, i64)>),
    Empty,
}
#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub index: u8,
    pub serializer: usize,
    pub value: Value,
}
#[derive(Clone, Debug, Serialize)]
pub struct Unsupported {
    pub index: u8,
    pub serializer: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct Packet {
    pub id: i32,
    /// No prefix is admitted when an unimplemented payload makes the update incomplete.
    pub entries: Vec<Entry>,
    pub unsupported: Option<Unsupported>,
}
/// Metadata notifications require no wire response, but unknown representations are never
/// silently treated as a valid visual update. The whole entity is marked incomplete instead.
pub(super) fn decode(r: &mut Reader<'_>) -> Result<Packet, Error> {
    let id = r.var()?;
    let mut entries = Vec::new();
    loop {
        let index = r.byte()?;
        if index == 255 {
            return Ok(Packet {
                id,
                entries,
                unsupported: None,
            });
        }
        let serializer = usize::try_from(r.var()?)
            .map_err(|_| Error::Invalid("negative metadata serializer"))?;
        let name = target::METADATA_SERIALIZERS
            .get(serializer)
            .copied()
            .ok_or(Error::Invalid("unknown metadata serializer"))?;
        let value = match name {
            "BYTE" => Value::Byte(r.byte()?),
            "ITEM_STACK" => Value::Item(super::super::inventory::stack(r)?),
            "INT"
            | "BLOCK_STATE"
            | "OPTIONAL_BLOCK_STATE"
            | "OPTIONAL_UNSIGNED_INT"
            | "DIRECTION"
            | "POSE"
            | "CAT_VARIANT"
            | "CAT_SOUND_VARIANT"
            | "COW_VARIANT"
            | "COW_SOUND_VARIANT"
            | "WOLF_VARIANT"
            | "WOLF_SOUND_VARIANT"
            | "FROG_VARIANT"
            | "PIG_VARIANT"
            | "PIG_SOUND_VARIANT"
            | "CHICKEN_VARIANT"
            | "CHICKEN_SOUND_VARIANT"
            | "ZOMBIE_NAUTILUS_VARIANT"
            | "SNIFFER_STATE"
            | "ARMADILLO_STATE"
            | "COPPER_GOLEM_STATE"
            | "WEATHERING_COPPER_STATE"
            | "HUMANOID_ARM"
            | "DYE_COLOR" => Value::Int(r.var()?),
            "LONG" => Value::Long(r.varlong()? as i64),
            "FLOAT" => Value::Float(r.f32()?),
            "STRING" => Value::String(r.string(32767)?),
            "COMPONENT" => Value::Component(Some(nbt::read(r)?)),
            "OPTIONAL_COMPONENT" => {
                Value::Component(if r.bool()? { Some(nbt::read(r)?) } else { None })
            }
            "BOOLEAN" => Value::Bool(r.bool()?),
            "ROTATIONS" | "VECTOR3" => Value::Vector(vec![r.f32()?, r.f32()?, r.f32()?]),
            "QUATERNION" => Value::Vector(vec![r.f32()?, r.f32()?, r.f32()?, r.f32()?]),
            "BLOCK_POS" => Value::Position(Some(r.i64()?)),
            "OPTIONAL_BLOCK_POS" => Value::Position(if r.bool()? { Some(r.i64()?) } else { None }),
            "OPTIONAL_LIVING_ENTITY_REFERENCE" => Value::Uuid(if r.bool()? {
                Some(r.uuid()?.into_bytes())
            } else {
                None
            }),
            "VILLAGER_DATA" => Value::Villager([r.var()?, r.var()?, r.var()?]),
            "OPTIONAL_GLOBAL_POS" => Value::GlobalPosition(if r.bool()? {
                Some((r.string(32767)?, r.i64()?))
            } else {
                None
            }),
            "PARTICLES" => {
                let count = r.var()?;
                if count < 0 {
                    return Err(Error::Invalid("negative metadata particle count"));
                }
                if count != 0 {
                    return Ok(deferred(r, id, index, name));
                }
                Value::Empty
            }
            // Particles, profiles and inline painting holders
            // need their own codecs. This known passive packet is retained as incomplete evidence.
            _ => {
                return Ok(deferred(r, id, index, name));
            }
        };
        entries.push(Entry {
            index,
            serializer,
            value,
        });
    }
}

fn deferred(r: &mut Reader<'_>, id: i32, index: u8, serializer: &'static str) -> Packet {
    r.0 = &[];
    Packet {
        id,
        entries: Vec::new(),
        unsupported: Some(Unsupported { index, serializer }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_metadata_indexes_and_serializers_are_independent_wire_vectors() {
        // Native protocol-777 pig: flags index0/byte0, baby index16/bool8, variant index19/type28.
        let bytes = [42, 0, 0, 0x20, 16, 8, 1, 19, 28, 2, 255];
        let mut r = Reader(&bytes);
        let packet = decode(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!(packet.id, 42);
        assert!(packet.unsupported.is_none());
        assert_eq!(
            packet
                .entries
                .iter()
                .map(|e| e.value.clone())
                .collect::<Vec<_>>(),
            [Value::Byte(32), Value::Bool(true), Value::Int(2)]
        );
        for end in 0..bytes.len() {
            assert!(decode(&mut Reader(&bytes[..end])).is_err());
        }
        assert!(decode(&mut Reader(&[42, 0, 8, 2, 255])).is_err());
        assert!(decode(&mut Reader(&[42, 0, 44, 0, 255])).is_err());
        assert_eq!(
            target::METADATA_PIG
                .iter()
                .find(|e| e.name == "DATA_VARIANT_ID")
                .map(|e| (e.index, e.serializer)),
            Some((19, 28))
        );
    }
    #[test]
    fn unsupported_complex_metadata_is_explicit_and_never_admits_a_prefix() {
        let particles = target::METADATA_SERIALIZERS
            .iter()
            .position(|n| *n == "PARTICLES")
            .unwrap() as u8;
        let bytes = [42, 0, 0, 32, 8, particles, 1, 255];
        let mut r = Reader(&bytes);
        let packet = decode(&mut r).unwrap();
        assert!(packet.entries.is_empty());
        assert_eq!(packet.unsupported.unwrap().serializer, "PARTICLES");
        r.finish().unwrap();
        // Previously deferred ITEM_STACK is implemented; malformed/truncated data must fail.
        assert!(decode(&mut Reader(&[42, 0, 0, 32, 8, 7, 1, 255])).is_err());
    }
    #[test]
    fn native_ground_stack_codec_vector_is_complete_and_empty_is_distinct() {
        // GroundReference, verified26.3 server ClientboundSetEntityDataPacket codec:
        // entity42, ItemEntity.DATA_ITEM8/ITEM_STACK7, dirt55 count5, empty patch, terminator.
        let mut reader = Reader(&[0x2a, 8, 7, 5, 0x37, 0, 0, 0xff]);
        let packet = decode(&mut reader).unwrap();
        reader.finish().unwrap();
        assert!(packet.unsupported.is_none());
        assert_eq!(packet.entries.len(), 1);
        let Value::Item(Some(item)) = &packet.entries[0].value else {
            panic!("missing ground stack")
        };
        assert_eq!(
            (
                packet.id,
                packet.entries[0].index,
                packet.entries[0].serializer,
                item.item,
                item.count
            ),
            (42, 8, 7, 55, 5)
        );
        assert!(item.added.is_empty() && item.removed.is_empty());
        let mut empty = Reader(&[42, 8, 7, 0, 255]);
        let packet = decode(&mut empty).unwrap();
        empty.finish().unwrap();
        assert_eq!(packet.entries[0].value, Value::Item(None));
        assert!(decode(&mut Reader(&[42, 8, 7, 5, 55, 0])).is_err());
    }
}
