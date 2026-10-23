use super::*;
use std::collections::{BTreeMap, BTreeSet};
/// A worker publication must match both the dimension and this column revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationKey {
    pub session: u64,
    pub epoch: u64,
    pub chunk: [i32; 2],
    pub revision: u64,
    pub neighbours: [Option<u64>; 8],
}
#[derive(Debug, Default)]
pub struct Terrain {
    dimension: Option<Dimension>,
    columns: BTreeMap<[i32; 2], Arc<Column>>,
    dirty: BTreeSet<[i32; 2]>,
    revision: u64,
    pub center: Option<[i32; 2]>,
    pub radius: Option<u32>,
    pub ignored_nonresident_updates: u64,
}
impl Terrain {
    /// Replaces all authority, including lighting, on every respawn/dimension entry.
    pub fn begin(&mut self, dimension: Dimension) {
        *self = Self {
            dimension: Some(dimension),
            ..Self::default()
        };
    }
    pub fn dimension(&self) -> Option<&Dimension> {
        self.dimension.as_ref()
    }
    pub fn column(&self, key: [i32; 2]) -> Option<&Arc<Column>> {
        self.columns.get(&key)
    }
    pub fn columns(&self) -> impl Iterator<Item = ([i32; 2], &Arc<Column>)> {
        self.columns.iter().map(|(k, v)| (*k, v))
    }
    pub fn len(&self) -> usize {
        self.columns.len()
    }
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }
    pub fn accepts(&self, session: u64, epoch: u64) -> bool {
        self.dimension
            .as_ref()
            .is_some_and(|d| d.session == session && d.epoch == epoch)
    }
    pub fn publication_key(&self, chunk: [i32; 2]) -> Option<PublicationKey> {
        let d = self.dimension.as_ref()?;
        Some(PublicationKey {
            session: d.session,
            epoch: d.epoch,
            chunk,
            revision: self.columns.get(&chunk)?.revision,
            neighbours: [
                (-1, -1),
                (-1, 0),
                (-1, 1),
                (0, -1),
                (0, 1),
                (1, -1),
                (1, 0),
                (1, 1),
            ]
            .map(|(x, z)| {
                let key = [chunk[0].checked_add(x)?, chunk[1].checked_add(z)?];
                self.columns.get(&key).map(|c| c.revision)
            }),
        })
    }
    pub fn is_current(&self, key: &PublicationKey) -> bool {
        self.publication_key(key.chunk).as_ref() == Some(key)
    }
    fn next_revision(&mut self) -> u64 {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("Java column revision exhausted");
        self.revision
    }
    fn mark_dirty(&mut self, [x, z]: [i32; 2]) {
        for (dx, dz) in (-1..=1).flat_map(|x| (-1..=1).map(move |z| (x, z))) {
            if let (Some(x), Some(z)) = (x.checked_add(dx), z.checked_add(dz)) {
                self.dirty.insert([x, z]);
            }
        }
    }
    /// Atomically replaces a complete server column; no unknown block is converted to air.
    pub fn replace(&mut self, key: [i32; 2], mut column: Column) -> bool {
        let Some(d) = &self.dimension else {
            return false;
        };
        let sections = (d.height / 16) as usize;
        if column.sections.len() != sections
            || !column.light.valid_for(sections)
            || column
                .sections
                .iter()
                .any(|s| s.blocks.len() != 4096 || s.biomes.len() != 64)
        {
            return false;
        }
        column.revision = self.next_revision();
        self.columns.insert(key, Arc::new(column));
        self.mark_dirty(key);
        true
    }
    pub fn unload(&mut self, key: [i32; 2]) {
        if self.columns.remove(&key).is_some() {
            self.mark_dirty(key);
        }
    }
    pub fn block(&self, [x, y, z]: [i32; 3]) -> Option<BlockStateId> {
        let d = self.dimension.as_ref()?;
        let y = y.checked_sub(d.min_y)?;
        if y < 0 || y >= d.height {
            return None;
        }
        let c = self.columns.get(&[x.div_euclid(16), z.div_euclid(16)])?;
        c.sections[y as usize / 16].blocks.get(
            ((y as usize % 16) << 8)
                | ((z.rem_euclid(16) as usize) << 4)
                | x.rem_euclid(16) as usize,
        )
    }
    pub fn set_block(&mut self, [x, y, z]: [i32; 3], state: BlockStateId) {
        let Some(d) = &self.dimension else {
            return;
        };
        let Some(local_y) = y.checked_sub(d.min_y).filter(|y| *y >= 0 && *y < d.height) else {
            return;
        };
        let key = [x.div_euclid(16), z.div_euclid(16)];
        if !self.columns.contains_key(&key) {
            self.ignored_nonresident_updates += 1;
            return;
        }
        if self.block([x, y, z]) == Some(state) {
            return;
        }
        let revision = self.next_revision();
        let column = Arc::make_mut(self.columns.get_mut(&key).unwrap());
        let section = Arc::make_mut(&mut column.sections[local_y as usize / 16]);
        section.blocks.set(
            ((local_y as usize % 16) << 8)
                | ((z.rem_euclid(16) as usize) << 4)
                | x.rem_euclid(16) as usize,
            state,
        );
        column.revision = revision;
        // The next full chunk supplies replacement heightmaps/block-entity data. Retaining these
        // across an authoritative block replacement would expose facts about the previous block.
        column.heightmaps.clear();
        column.block_entities.retain(|b| b.position != [x, y, z]);
        self.mark_dirty(key);
    }
    pub fn update_light(&mut self, key: [i32; 2], update: Light) -> bool {
        let Some(column) = self.columns.get(&key) else {
            self.ignored_nonresident_updates += 1;
            return false;
        };
        if !update.valid_for(column.sections.len()) {
            return false;
        }
        let revision = self.next_revision();
        let column = Arc::make_mut(self.columns.get_mut(&key).unwrap());
        for (old, new) in column
            .light
            .sky
            .iter_mut()
            .zip(update.sky)
            .chain(column.light.block.iter_mut().zip(update.block))
        {
            if new.is_some() {
                *old = new;
            }
        }
        column.revision = revision;
        self.mark_dirty(key);
        true
    }
    pub fn update_biomes(&mut self, key: [i32; 2], biomes: Vec<Palette<BiomeId>>) -> bool {
        let Some(column) = self.columns.get(&key) else {
            self.ignored_nonresident_updates += 1;
            return false;
        };
        if biomes.len() != column.sections.len() || biomes.iter().any(|b| b.len() != 64) {
            return false;
        }
        let revision = self.next_revision();
        let column = Arc::make_mut(self.columns.get_mut(&key).unwrap());
        for (section, biome) in column.sections.iter_mut().zip(biomes) {
            Arc::make_mut(section).biomes = biome;
        }
        column.revision = revision;
        self.mark_dirty(key);
        true
    }
    pub fn take_dirty(&mut self) -> BTreeSet<[i32; 2]> {
        std::mem::take(&mut self.dirty)
    }
    /// Packed terrain/light payload bytes, excluding allocator overhead and registry/asset data.
    pub fn retained_payload_bytes(&self) -> usize {
        self.columns
            .values()
            .map(|c| {
                c.sections
                    .iter()
                    .map(|s| s.blocks.retained_bytes() + s.biomes.retained_bytes())
                    .sum::<usize>()
                    + c.light
                        .sky
                        .iter()
                        .chain(&c.light.block)
                        .filter_map(|l| l.as_ref())
                        .map(|l| l.len())
                        .sum::<usize>()
                    + c.heightmaps.iter().map(|(_, v)| v.len() * 8).sum::<usize>()
            })
            .sum()
    }
}
