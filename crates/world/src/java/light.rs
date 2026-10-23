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
        let block = match layer(&self.block, section) {
            Some(bytes) => nibble(bytes, index)?,
            None => 0,
        };
        let sky = if !dimension.has_skylight {
            0
        } else if let Some(bytes) = layer(&self.sky, section) {
            nibble(bytes, index)?
        } else {
            let above = (section + 1).max(0) as usize;
            match self.sky.iter().skip(above).flatten().next() {
                Some(bytes) => nibble(bytes, row)?,
                None => 15,
            }
        };
        Some([block, sky])
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

fn nibble(bytes: &[u8], index: usize) -> Option<u8> {
    match bytes.len() {
        0 => Some(0),
        2048 => bytes.get(index / 2).map(|b| (b >> ((index & 1) * 4)) & 15),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
