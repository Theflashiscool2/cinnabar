//! Java26.3 Brigadier server completions. Ranges use UTF-16 code units, not UTF-8 bytes.
use crate::{ProtocolError as Error, codec, nbt, wire::Reader};

#[derive(Clone, Debug, serde::Serialize)]
pub struct Entry {
    pub text: String,
    pub tooltip: Option<nbt::Value>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Packet {
    pub id: i32,
    pub start: i32,
    pub length: i32,
    pub entries: Vec<Entry>,
}
/// Native request includes the slash and only text preceding the editor's cursor.
pub fn request(id: i32, input: &str) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    codec::write_varint(id, &mut out);
    codec::write_string(input, 32500, &mut out)?;
    Ok(out)
}
pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Packet, Error> {
    let id = r.var()?;
    let start = r.var()?;
    let length = r.var()?;
    if start < 0 || length < 0 || start.checked_add(length).is_none() {
        return Err(Error::Invalid("Java command suggestion range"));
    }
    // The packet framing bounds both the count and total allocations; no small
    // client-imposed count cap may discard legitimate command catalogs.
    let count = r.count(r.0.len() / 2)?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(Entry {
            text: r.string(32767)?,
            tooltip: if r.bool()? { Some(nbt::read(r)?) } else { None },
        });
    }
    Ok(Packet {
        id,
        start,
        length,
        entries,
    })
}

/// Only the latest request may publish; a late response cannot overwrite a newer
/// admitted response before the presentation thread reads it.
#[derive(Default)]
pub(crate) struct Pending(Option<i32>);
impl Pending {
    pub fn begin(&mut self, id: i32) {
        self.0 = Some(id);
    }
    pub fn accept(&mut self, id: i32) -> bool {
        if self.0 == Some(id) {
            self.0 = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn responses_cannot_overwrite_newer_queries_or_duplicate_publications() {
        let mut state = Pending::default();
        state.begin(1);
        state.begin(2);
        assert!(!state.accept(1));
        assert!(state.accept(2));
        assert!(!state.accept(1));
        assert!(!state.accept(2));
        state.begin(3);
        state = Pending::default();
        assert!(!state.accept(3));
    }
    #[test]
    fn native_suggestions_preserve_ranges_tooltips_and_truncation_checks() {
        assert_eq!(request(7, "/he").unwrap(), b"\x07\x03/he");
        // Native Entry's optional trusted component uses anonymous network NBT.
        let bytes = b"\x07\x01\x02\x02\x04help\x00\x05hello\x01\x08\x00\x03Tip";
        let mut r = Reader(bytes);
        let packet = decode(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!((packet.id, packet.start, packet.length), (7, 1, 2));
        assert_eq!(packet.entries[0].text, "help");
        assert!(matches!(&packet.entries[1].tooltip, Some(nbt::Value::String(s)) if s == "Tip"));
        for end in 0..bytes.len() {
            assert!(decode(&mut Reader(&bytes[..end])).is_err());
        }
    }
}
