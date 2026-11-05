//! Native26.3 SetEquipment: continuation bit on the ordinal, then the SAME item
//! component codec as containers. Parse the whole packet before admission.
use super::super::inventory;
use crate::{ProtocolError as Error, wire::Reader};
use std::collections::BTreeMap;
/// EquipmentSlot.VALUES in the pinned archive (including BODY and SADDLE).
pub const SLOT_COUNT: usize = 8;
#[derive(Clone, Debug, serde::Serialize)]
pub struct Packet {
    pub id: i32,
    /// Filled by the socket's existing spawn owner, never guessed by the decoder.
    pub lifetime: u64,
    pub slots: BTreeMap<u8, Option<inventory::Stack>>,
}
pub(super) fn decode(r: &mut Reader<'_>) -> Result<Packet, Error> {
    let id = r.var()?;
    let mut slots = BTreeMap::new();
    loop {
        let encoded = r.byte()?;
        let slot = encoded & 0x7f;
        if usize::from(slot) >= SLOT_COUNT {
            return Err(Error::Invalid("Java equipment slot ordinal"));
        }
        // Native permits repeated ordinals; its ordered setter leaves the last value.
        // Collapsing here bounds retained storage to eight slots without rejecting repeats.
        slots.insert(slot, inventory::stack(r)?);
        if encoded & 0x80 == 0 {
            break;
        }
    }
    Ok(Packet {
        id,
        lifetime: 0,
        slots,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_continuation_clear_last_wins_and_no_partial_packet() {
        let mut r = Reader(&[0xac, 2, 0x80, 0, 0x87, 0, 0, 0]);
        let p = decode(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!(p.id, 300);
        assert_eq!(p.lifetime, 0);
        assert_eq!(p.slots.len(), 2);
        assert_eq!(p.slots[&0], None);
        assert_eq!(p.slots[&7], None);
        for bytes in [&[1][..], &[1, 0x80, 0][..], &[1, 8, 0][..], &[1, 0, 1][..]] {
            assert!(decode(&mut Reader(bytes)).is_err(), "{bytes:?}");
        }
    }
}
