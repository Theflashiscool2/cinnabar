//! `net.minecraft.util.HashOps` over Guava's CRC32C: the canonical hash of a
//! codec-encoded value that `HashedStack` sends for each data component.
//!
//! Vanilla runs a `Codec` against `HashOps` (a `DynamicOps<HashCode>`), so every
//! value is hashed as its codec form: a tag byte per type, map entries sorted by
//! their hashes, lists in order. The functions here are `HashOps`' `create*`
//! methods and its record builder; a component's codec form is built from them.
// Adapted under MIT; copyright/provenance: assets/licenses/Native-Java-Reference-MIT.txt.

/// Guava `Hashing.crc32c()`: the CRC-32C (Castagnoli) of the bytes.
pub fn crc32c(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut crc = i as u32;
            let mut bit = 0;
            while bit < 8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0x82F6_3B78
                } else {
                    crc >> 1
                };
                bit += 1;
            }
            table[i] = crc;
            i += 1;
        }
        table
    };
    let mut crc = !0u32;
    for &b in bytes {
        crc = TABLE[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

/// A Guava `HashCode` of CRC32C: four bytes (`IntHashCode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HashCode(pub u32);
impl HashCode {
    /// `HashCode.asInt`: what `HashedPatchMap` sends for a component.
    pub fn as_int(self) -> i32 {
        self.0 as i32
    }
    /// `HashCode.asBytes`: little-endian, as `IntHashCode` stores it.
    pub fn as_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }
}

/// A Guava `Hasher`: little-endian primitives, then the CRC of everything put.
#[derive(Default)]
struct Hasher(Vec<u8>);
impl Hasher {
    fn byte(mut self, v: u8) -> Self {
        self.0.push(v);
        self
    }
    fn bytes(mut self, v: &[u8]) -> Self {
        self.0.extend_from_slice(v);
        self
    }
    fn hash(self) -> HashCode {
        HashCode(crc32c(&self.0))
    }
}

/// `HashOps.CRC32C_INSTANCE`.
pub struct HashOps;
impl HashOps {
    /// `HashOps.empty`: the hash of the payload `[TAG_EMPTY]`.
    pub fn empty() -> HashCode {
        Hasher::default().byte(1).hash()
    }
    pub fn empty_map() -> HashCode {
        Hasher::default().byte(2).byte(3).hash()
    }
    pub fn empty_list() -> HashCode {
        Hasher::default().byte(4).byte(5).hash()
    }
    pub fn create_byte(v: i8) -> HashCode {
        Hasher::default().byte(6).byte(v as u8).hash()
    }
    pub fn create_short(v: i16) -> HashCode {
        Hasher::default().byte(7).bytes(&v.to_le_bytes()).hash()
    }
    pub fn create_int(v: i32) -> HashCode {
        Hasher::default().byte(8).bytes(&v.to_le_bytes()).hash()
    }
    pub fn create_long(v: i64) -> HashCode {
        Hasher::default().byte(9).bytes(&v.to_le_bytes()).hash()
    }
    /// From the float's raw bits (`putFloat` writes `floatToRawIntBits`).
    pub fn create_float(bits: u32) -> HashCode {
        Hasher::default().byte(10).bytes(&bits.to_le_bytes()).hash()
    }
    pub fn create_double(bits: u64) -> HashCode {
        Hasher::default().byte(11).bytes(&bits.to_le_bytes()).hash()
    }
    pub fn create_f32(v: f32) -> HashCode {
        Self::create_float(v.to_bits())
    }
    /// `createString`: the length in UTF-16 units, then each unit (`putUnencodedChars`).
    pub fn create_string_units(units: &[u16]) -> HashCode {
        let mut h = Hasher::default()
            .byte(12)
            .bytes(&(units.len() as i32).to_le_bytes());
        for u in units {
            h = h.bytes(&u.to_le_bytes());
        }
        h.hash()
    }
    pub fn create_string(v: &str) -> HashCode {
        let units: Vec<u16> = v.encode_utf16().collect();
        Self::create_string_units(&units)
    }
    pub fn create_boolean(v: bool) -> HashCode {
        Hasher::default().byte(13).byte(v as u8).hash()
    }
    /// `createMap`: the entries sorted by key hash, then value hash (both as
    /// unsigned ints, `HashCode.padToLong`).
    pub fn create_map(mut entries: Vec<(HashCode, HashCode)>) -> HashCode {
        entries.sort_by_key(|(k, v)| (k.0, v.0));
        let mut h = Hasher::default().byte(2);
        for (k, v) in entries {
            h = h.bytes(&k.as_bytes()).bytes(&v.as_bytes());
        }
        h.byte(3).hash()
    }
    pub fn create_list(values: impl IntoIterator<Item = HashCode>) -> HashCode {
        let mut h = Hasher::default().byte(4);
        for v in values {
            h = h.bytes(&v.as_bytes());
        }
        h.byte(5).hash()
    }
    pub fn create_byte_list(values: &[u8]) -> HashCode {
        Hasher::default().byte(14).bytes(values).byte(15).hash()
    }
    pub fn create_int_list(values: &[i32]) -> HashCode {
        let mut h = Hasher::default().byte(16);
        for v in values {
            h = h.bytes(&v.to_le_bytes());
        }
        h.byte(17).hash()
    }
    pub fn create_long_list(values: &[i64]) -> HashCode {
        let mut h = Hasher::default().byte(18);
        for v in values {
            h = h.bytes(&v.to_le_bytes());
        }
        h.byte(19).hash()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_hashops_vectors_and_crc32c_check_value() {
        assert_eq!(crc32c(b"123456789"), 0xe3069283);
        assert_eq!(HashOps::create_int(1).as_int(), 1565579036);
        assert_eq!(HashOps::create_string("ab").as_int(), 1228822133);
        assert_eq!(HashOps::create_f32(1.0).as_int(), 1694772624);
        assert_eq!(HashOps::create_boolean(true).as_int(), -1019818302);
        assert_eq!(
            HashOps::create_list([1, 2, 3].map(HashOps::create_int)).as_int(),
            -1179931408
        );
        assert_eq!(HashOps::empty_map().as_int(), -982207288);
        let a = (HashOps::create_string("a"), HashOps::create_int(1));
        let b = (HashOps::create_string("b"), HashOps::create_int(2));
        assert_eq!(
            HashOps::create_map(vec![a, b]),
            HashOps::create_map(vec![b, a])
        );
        assert_ne!(
            HashOps::create_list([a.1, b.1]),
            HashOps::create_list([b.1, a.1])
        );
        assert_eq!(
            HashOps::create_string("😀"),
            HashCode(crc32c(&[12, 2, 0, 0, 0, 0x3d, 0xd8, 0, 0xde]))
        );
    }
}
