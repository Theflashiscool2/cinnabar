//! Native LastSeenMessagesTracker ring and MessageSignatureCache, owned by one connection.
use crate::{ProtocolError as Error, codec};
use std::collections::VecDeque;
pub const SIGNATURE_BYTES: usize = 256;
pub type Signature = [u8; SIGNATURE_BYTES];
pub(super) const WINDOW: usize = 20;
const CACHE: usize = 128;
#[derive(Clone, Copy)]
struct Entry {
    signature: Signature,
    pending: bool,
}
#[derive(Clone)]
pub(super) struct Window {
    entries: [Option<Entry>; WINDOW],
    tail: usize,
    offset: i32,
    last: Option<Signature>,
}
impl Default for Window {
    fn default() -> Self {
        Self {
            entries: [None; WINDOW],
            tail: 0,
            offset: 0,
            last: None,
        }
    }
}
impl Window {
    /// Only real received signatures advance the ring; consecutive duplicates do not.
    pub fn observe(&mut self, signature: Signature, displayed: bool) -> bool {
        if self.last == Some(signature) {
            return false;
        }
        self.last = Some(signature);
        self.entries[self.tail] = displayed.then_some(Entry {
            signature,
            pending: true,
        });
        self.tail = (self.tail + 1) % WINDOW;
        self.offset += 1;
        true
    }
    pub fn ignore(&mut self, signature: &Signature) {
        for entry in &mut self.entries {
            if entry
                .as_ref()
                .is_some_and(|e| e.pending && &e.signature == signature)
            {
                *entry = None;
            }
        }
    }
    pub fn offset(&self) -> i32 {
        self.offset
    }
    /// Standalone acknowledgements clear the offset, not pending/displayed entries.
    pub fn take_offset(&mut self) -> i32 {
        std::mem::take(&mut self.offset)
    }
    pub fn update(&mut self) -> Vec<u8> {
        self.update_with_signatures().0
    }
    pub fn update_with_signatures(&mut self) -> (Vec<u8>, Vec<Signature>) {
        let mut signatures = Vec::new();
        let mut bytes = Vec::with_capacity(8);
        codec::write_varint(self.take_offset(), &mut bytes);
        let mut mask = 0u32;
        let mut checksum = 1i32;
        for i in 0..WINDOW {
            if let Some(entry) = &mut self.entries[(self.tail + i) % WINDOW] {
                mask |= 1 << i;
                signatures.push(entry.signature);
                entry.pending = false;
                let hash = entry.signature.iter().fold(1i32, |hash, byte| {
                    hash.wrapping_mul(31).wrapping_add(*byte as i8 as i32)
                });
                checksum = checksum.wrapping_mul(31).wrapping_add(hash);
            }
        }
        bytes.extend_from_slice(&mask.to_le_bytes()[..WINDOW.div_ceil(8)]);
        // Zero is the protocol's ignore-checksum sentinel, never a computed checksum.
        bytes.push(if checksum as u8 == 0 {
            1
        } else {
            checksum as u8
        });
        (bytes, signatures)
    }
}
pub(super) struct Cache([Option<Signature>; CACHE]);
impl Default for Cache {
    fn default() -> Self {
        Self([None; CACHE])
    }
}
impl Cache {
    pub fn get(&self, index: i32) -> Result<Signature, Error> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.0.get(i))
            .copied()
            .flatten()
            .ok_or(Error::Invalid("unresolved Java chat signature reference"))
    }
    /// Native push preserves incoming duplicates and evicts displaced old entries in order.
    pub fn push(&mut self, last_seen: &[Signature], signature: Option<Signature>) {
        let mut incoming: VecDeque<_> = last_seen.iter().copied().chain(signature).collect();
        let fresh: Vec<_> = incoming.iter().copied().collect();
        for slot in &mut self.0 {
            let Some(next) = incoming.pop_back() else {
                break;
            };
            if let Some(old) = slot.replace(next)
                && !fresh.contains(&old)
            {
                incoming.push_front(old);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sig(n: u8) -> Signature {
        let mut s = [0; SIGNATURE_BYTES];
        s[0] = n;
        s
    }
    #[test]
    fn native_26_3_ring_vectors() {
        // Independent official LastSeenMessagesTracker/Update.STREAM_CODEC probe output.
        let mut w = Window::default();
        assert_eq!(w.update(), [0, 0, 0, 0, 1]);
        assert!(w.observe(sig(1), true));
        assert_eq!(w.update(), [1, 0, 0, 8, 255]);
        assert!(!w.observe(sig(1), false));
        w.ignore(&sig(1));
        assert_eq!(w.update(), [0, 0, 0, 8, 255]);
        w.observe(sig(2), false);
        assert_eq!(w.update(), [1, 0, 0, 4, 255]);
        w.observe(sig(3), true);
        w.ignore(&sig(3));
        assert_eq!(w.update(), [1, 0, 0, 2, 255]);
        let mut w = Window::default();
        for n in 1..=21 {
            w.observe(sig(n), true);
        }
        assert_eq!(w.update(), [21, 255, 255, 15, 247]);
        assert_eq!(w.take_offset(), 0);
        assert_eq!(w.update(), [0, 255, 255, 15, 247]);
        let mut w = Window::default();
        w.observe(sig(255), true);
        assert_eq!(w.offset(), 1);
        assert_eq!(w.take_offset(), 1);
        w.ignore(&sig(255));
        assert_eq!(w.update(), [0, 0, 0, 0, 1]);
        let mut w = Window::default();
        let mut s = [0; SIGNATURE_BYTES];
        s[255] = 224;
        w.observe(s, true);
        assert_eq!(w.update(), [1, 0, 0, 8, 1]);
    }
    #[test]
    fn cache_references_eviction_and_duplicates() {
        let mut c = Cache::default();
        assert!(c.get(0).is_err());
        c.push(&[sig(1), sig(2)], Some(sig(3)));
        assert_eq!(c.get(0).unwrap(), sig(3));
        assert_eq!(c.get(2).unwrap(), sig(1));
        c.push(&[sig(2), sig(2)], Some(sig(4)));
        assert_eq!(c.get(0).unwrap(), sig(4));
        assert_eq!(c.get(1).unwrap(), sig(2));
        assert_eq!(c.get(2).unwrap(), sig(2));
        assert_eq!(c.get(3).unwrap(), sig(3));
        for n in 5..=140 {
            c.push(&[], Some(sig(n)));
        }
        assert_eq!(c.get(0).unwrap(), sig(140));
        assert_eq!(c.get(127).unwrap(), sig(13));
        assert!(c.get(-1).is_err());
        assert!(c.get(128).is_err());
    }
}
