//! Native light lookup shared by terrain meshes and actor presentation.
use super::{Dimension, Light, Terrain};
use std::sync::Arc;

impl Light {
    /// Samples a resident column, including boundary sections and unallocated storage.
    /// Matches the pinned client's BlockLightSectionStorage and SkyLightSectionStorage
    /// visible-data lookup: missing block layers are dark; missing sky layers inherit
    /// the bottom row above, or full sky when no higher layer exists.
    pub fn sample(&self, dimension: &Dimension, [x, y, z]: [i32; 3]) -> Option<[u8; 2]> {
        let section = i64::from(y.div_euclid(16)) - i64::from(dimension.min_y.div_euclid(16)) + 1;
        let row = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
        let index = y.rem_euclid(16) as usize * 256 + row;
        let [block, sky] = self.sources(dimension, section);
        Some([block.sample(index, row)?, sky.sample(index, row)?])
    }

    fn sources(&self, dimension: &Dimension, section: i64) -> [Source<'_>; 2] {
        let block = layer(&self.block, section)
            .map_or(Source::Constant(0), |bytes| Source::new(bytes, false));
        let sky = if !dimension.has_skylight {
            Source::Constant(0)
        } else if let Some(bytes) = layer(&self.sky, section) {
            Source::new(bytes, false)
        } else {
            let above = (section + 1).max(0) as usize;
            self.sky
                .iter()
                .skip(above)
                .flatten()
                .next()
                .map_or(Source::Constant(15), |bytes| Source::new(bytes, true))
        };
        [block, sky]
    }

    /// Includes boundary light layers sampled by adjacent terrain. Inherited sky compares only
    /// its effective bottom row, so unrelated rows above cannot invalidate an entire column.
    pub(super) fn changed_sections(&self, next: &Self, dimension: &Dimension) -> Vec<i32> {
        (0..i64::from(dimension.height / 16) + 2)
            .filter(|&section| self.sources(dimension, section) != next.sources(dimension, section))
            .map(|section| dimension.min_y.div_euclid(16) + section as i32 - 1)
            .collect()
    }

    /// Rejects malformed arrays before storage changes; an empty array encodes zero light.
    pub(super) fn valid_for(&self, sections: usize) -> bool {
        self.sky.len() == sections + 2
            && self.block.len() == sections + 2
            && self
                .sky
                .iter()
                .chain(&self.block)
                .flatten()
                .all(|bytes| bytes.is_empty() || bytes.len() == 2048)
    }
}

impl Terrain {
    /// Missing columns remain unavailable; only resident columns have native light defaults.
    pub fn light(&self, position: [i32; 3]) -> Option<[u8; 2]> {
        let dimension = self.dimension()?;
        let [x, _, z] = position;
        self.column([x.div_euclid(16), z.div_euclid(16)])?
            .light
            .sample(dimension, position)
    }
}

fn layer(layers: &[Option<Arc<[u8]>>], section: i64) -> Option<&[u8]> {
    layers.get(usize::try_from(section).ok()?)?.as_deref()
}

#[derive(PartialEq, Eq)]
enum Source<'a> {
    Constant(u8),
    Layer(&'a [u8]),
    BottomRow(&'a [u8]),
    Invalid,
}
impl<'a> Source<'a> {
    fn new(bytes: &'a [u8], bottom_row: bool) -> Self {
        match bytes.len() {
            0 => Self::Constant(0),
            2048 if bottom_row => Self::BottomRow(&bytes[..128]),
            2048 => Self::Layer(bytes),
            _ => Self::Invalid,
        }
    }
    fn sample(&self, index: usize, row: usize) -> Option<u8> {
        let (bytes, index) = match self {
            Self::Constant(value) => return Some(*value),
            Self::Layer(bytes) => (*bytes, index),
            Self::BottomRow(bytes) => (*bytes, row),
            Self::Invalid => return None,
        };
        bytes.get(index / 2).map(|b| (b >> ((index & 1) * 4)) & 15)
    }
}

#[cfg(test)]
mod tests;
