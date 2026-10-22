//! Protocol-777 paletted containers: padded big-endian longs, with no array-length prefix.
use crate::{ProtocolError as Error, wire::Reader};

/// A validated container in Java Y/Z/X order. IDs belong to its Java registry only.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Palette {
    pub bits: u8,
    pub values: Vec<u32>,
    pub words: Vec<u64>,
    pub len: usize,
}
impl Palette {
    pub fn read(r: &mut Reader<'_>, len: usize, indirect_max: u8) -> Result<Self, Error> {
        let bits = r.byte()?;
        if bits > 31 {
            return Err(Error::Invalid("Java palette bit width"));
        }
        let mut values = Vec::new();
        if bits == 0 {
            values.push(nonnegative(r)?);
        } else if bits <= indirect_max {
            let n = r.count(1usize << bits)?;
            if n == 0 {
                return Err(Error::Invalid("empty Java palette"));
            }
            for _ in 0..n {
                values.push(nonnegative(r)?);
            }
        }
        let n = if bits == 0 {
            0
        } else {
            len.div_ceil(64 / usize::from(bits))
        };
        let words = r
            .take(n * 8)?
            .as_chunks::<8>()
            .0
            .iter()
            .map(|b| u64::from_be_bytes(*b))
            .collect();
        let palette = Self {
            bits,
            values,
            words,
            len,
        };
        for index in 0..len {
            palette
                .get(index)
                .ok_or(Error::Invalid("Java palette index"))?;
        }
        Ok(palette)
    }
    pub fn get(&self, index: usize) -> Option<u32> {
        if index >= self.len {
            return None;
        }
        if self.bits == 0 {
            return self.values.first().copied();
        }
        let per_word = 64 / usize::from(self.bits);
        let id = ((self.words[index / per_word] >> ((index % per_word) * usize::from(self.bits)))
            & ((1u64 << self.bits) - 1)) as u32;
        if self.values.is_empty() {
            Some(id)
        } else {
            self.values.get(id as usize).copied()
        }
    }
    pub fn validate_registry(&self, size: usize) -> Result<(), Error> {
        for index in 0..self.len {
            if self.get(index).is_none_or(|id| id as usize >= size) {
                return Err(Error::Invalid("Java palette ID outside registry"));
            }
        }
        Ok(())
    }
}
pub(super) fn nonnegative(r: &mut Reader<'_>) -> Result<u32, Error> {
    u32::try_from(r.var()?).map_err(|_| Error::Invalid("negative Java registry ID"))
}
