//! Compact Java registry values. This representation never enters Bedrock's runtime-ID lookup.
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockStateId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BiomeId(pub u32);

#[derive(Clone, Debug)]
pub struct Palette<T> {
    bits: u8,
    values: Box<[T]>,
    words: Box<[u64]>,
    len: usize,
    edits: BTreeMap<usize, T>,
}
impl<T: Copy + From<u32>> Palette<T> {
    /// Takes already decoded registry values, checking packing dimensions before domain admission.
    pub fn new(bits: u8, values: Vec<T>, words: Vec<u64>, len: usize) -> Option<Self> {
        if bits > 31 || len == 0 {
            return None;
        }
        let n = if bits == 0 {
            0
        } else {
            len.div_ceil(64 / bits as usize)
        };
        if words.len() != n || (bits == 0 && values.len() != 1) {
            return None;
        }
        let p = Self {
            bits,
            values: values.into(),
            words: words.into(),
            len,
            edits: BTreeMap::new(),
        };
        Some(p)
    }
    pub fn uniform(value: T, len: usize) -> Self {
        Self {
            bits: 0,
            values: vec![value].into(),
            words: Box::new([]),
            len,
            edits: BTreeMap::new(),
        }
    }
    pub fn get(&self, index: usize) -> Option<T> {
        if index >= self.len {
            return None;
        }
        if let Some(value) = self.edits.get(&index) {
            return Some(*value);
        }
        if self.bits == 0 {
            return self.values.first().copied();
        }
        let per_word = 64 / self.bits as usize;
        let index = ((self.words[index / per_word] >> (index % per_word * self.bits as usize))
            & ((1u64 << self.bits) - 1)) as u32;
        if self.values.is_empty() {
            Some(T::from(index))
        } else {
            self.values.get(index as usize).copied()
        }
    }
    /// An authoritative cell edit preserves the packed base snapshot. At most len edits survive.
    pub fn set(&mut self, index: usize, value: T) -> bool {
        if index >= self.len {
            return false;
        }
        self.edits.insert(index, value);
        true
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of_val(&*self.values)
            + std::mem::size_of_val(&*self.words)
            + self.edits.len() * (std::mem::size_of::<T>() + 3 * std::mem::size_of::<usize>())
    }
}
impl From<u32> for BlockStateId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}
impl From<u32> for BiomeId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}
