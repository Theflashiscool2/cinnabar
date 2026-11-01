mod local_light;
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
/// Inclusive authoritative block/light changes, coalesced within one native section.
/// Consumers choose their own dependency halo; the store does not prescribe mesh work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangedRegion {
    pub min: [i32; 3],
    pub max: [i32; 3],
}
#[derive(Debug, Default)]
pub struct Terrain {
    dimension: Option<Dimension>,
    columns: BTreeMap<[i32; 2], Arc<Column>>,
    dirty: BTreeSet<[i32; 2]>,
    changed_regions: BTreeMap<[i32; 3], ChangedRegion>,
    revision: u64,
    block_events: BTreeMap<[i32; 3], BlockEvent>,
    block_types: Option<Arc<[u32]>>,
    block_generations: BTreeMap<[i32; 3], u64>,
    block_entity_revisions: BTreeMap<[i32; 3], u64>,
    pub pending_pistons: BTreeMap<[i32; 3], super::piston::Pending>,
    light_changes: BTreeSet<[i32; 3]>,
    pub pistons: BTreeMap<[i32; 3], super::piston::Moving>,
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
    fn mark_region(&mut self, region: ChangedRegion) {
        let section = region.min.map(|v| v.div_euclid(16));
        self.changed_regions
            .entry(section)
            .and_modify(|old| {
                for axis in 0..3 {
                    old.min[axis] = old.min[axis].min(region.min[axis]);
                    old.max[axis] = old.max[axis].max(region.max[axis]);
                }
            })
            .or_insert(region);
    }
    fn mark_light_section(&mut self, [x, z]: [i32; 2], y: i32) {
        let [Some(x), Some(y), Some(z)] = [x, y, z].map(|v| v.checked_mul(16)) else {
            self.mark_dirty([x, z]);
            return;
        };
        // Multiples of sixteen leave fifteen representable coordinates in every axis.
        self.mark_region(ChangedRegion {
            min: [x, y, z],
            max: [x + 15, y + 15, z + 15],
        });
    }
    fn mark_dirty(&mut self, [x, z]: [i32; 2]) {
        self.changed_regions.retain(|p, _| [p[0], p[2]] != [x, z]);
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
        self.pistons
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.pending_pistons
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.light_changes
            .retain(|p| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        column.incarnation = column.revision;
        self.block_entity_revisions
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.block_generations
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.block_events
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.columns.insert(key, Arc::new(column));
        self.mark_dirty(key);
        true
    }
    pub fn unload(&mut self, key: [i32; 2]) {
        self.pistons
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.pending_pistons
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.light_changes
            .retain(|p| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.block_entity_revisions
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.block_generations
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
        self.block_events
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != key);
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
        let old = self.block([x, y, z]);
        if old == Some(state) {
            return;
        }
        self.light_changes.insert([x, y, z]);
        let same_type = self.block_types.as_ref().is_some_and(|types| {
            let before = old.and_then(|id| types.get(id.0 as usize));
            before.is_some() && before == types.get(state.0 as usize)
        });
        if !same_type {
            self.pistons.remove(&[x, y, z]);
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
        if !same_type {
            column.block_entities.retain(|b| b.position != [x, y, z]);
            self.block_events.remove(&[x, y, z]);
            self.block_entity_revisions.remove(&[x, y, z]);
            self.block_generations.insert([x, y, z], revision);
        }
        self.mark_region(ChangedRegion {
            min: [x, y, z],
            max: [x, y, z],
        });
    }
    /// Version-verified native state-to-block identities; no model or edition translation.
    pub fn install_block_types(&mut self, types: Arc<[u32]>) {
        self.block_types = Some(types);
    }
    pub fn block_generation(&self, position: [i32; 3]) -> Option<u64> {
        let column = self
            .columns
            .get(&[position[0].div_euclid(16), position[2].div_euclid(16)])?;
        Some(
            self.block_generations
                .get(&position)
                .copied()
                .unwrap_or(column.incarnation),
        )
    }
    /// Receipt revision, including equal-value updates, separate from block identity and lighting.
    pub fn block_entity_revision(&self, position: [i32; 3]) -> Option<u64> {
        let column = self
            .columns
            .get(&[position[0].div_euclid(16), position[2].div_euclid(16)])?;
        column
            .block_entities
            .iter()
            .any(|e| e.position == position)
            .then(|| {
                self.block_entity_revisions
                    .get(&position)
                    .copied()
                    .unwrap_or(column.incarnation)
            })
    }
    /// Keeps one server event per resident position; no unbounded per-frame event queue.
    pub fn block_event(&self, position: [i32; 3]) -> Option<BlockEvent> {
        self.block_events.get(&position).copied()
    }
    pub fn set_block_event(
        &mut self,
        position: [i32; 3],
        block: u32,
        action: u8,
        value: u8,
    ) -> bool {
        if self.block(position).is_none() {
            self.ignored_nonresident_updates += 1;
            return false;
        }
        let revision = self.next_revision();
        let binary_transition = if value <= 1 {
            Some(value)
        } else {
            self.block_events
                .get(&position)
                .filter(|event| event.block == block && event.action == action)
                .and_then(|event| event.binary_transition)
        };
        self.block_events.insert(
            position,
            BlockEvent {
                block,
                action,
                value,
                binary_transition,
                revision,
            },
        );
        true
    }
    pub fn set_block_entity(&mut self, entity: BlockEntity) -> bool {
        if self.block(entity.position).is_none() {
            self.ignored_nonresident_updates += 1;
            return false;
        }
        let key = [
            entity.position[0].div_euclid(16),
            entity.position[2].div_euclid(16),
        ];
        let revision = self.next_revision();
        self.block_entity_revisions
            .insert(entity.position, revision);
        let column = Arc::make_mut(self.columns.get_mut(&key).expect("resident block"));
        if let Some(old) = column
            .block_entities
            .iter_mut()
            .find(|old| old.position == entity.position)
        {
            *old = entity;
        } else {
            column.block_entities.push(entity);
        }
        column.revision = revision;
        self.mark_dirty(key);
        true
    }
    pub fn update_light(&mut self, key: [i32; 2], update: Light) -> bool {
        let Some(column) = self.columns.get(&key) else {
            self.ignored_nonresident_updates += 1;
            return false;
        };
        if !update.valid_for(column.sections.len()) {
            return false;
        }
        if !column
            .light
            .sky
            .iter()
            .zip(&update.sky)
            .chain(column.light.block.iter().zip(&update.block))
            .any(|(old, new)| {
                new.as_ref()
                    .is_some_and(|new| old.as_deref() != Some(new.as_ref()))
            })
        {
            return true;
        }
        let mut merged = column.light.clone();
        for (old, new) in merged
            .sky
            .iter_mut()
            .zip(update.sky)
            .chain(merged.block.iter_mut().zip(update.block))
        {
            if new.is_some() {
                *old = new;
            }
        }
        let changed = column.light.changed_sections(
            &merged,
            self.dimension.as_ref().expect("resident dimension"),
        );
        let revision = self.next_revision();
        let column = Arc::make_mut(self.columns.get_mut(&key).unwrap());
        column.light = merged;
        column.revision = revision;
        for section in changed {
            self.mark_light_section(key, section);
        }
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
    /// Drains coalesced spatial changes separately from full column replacement/unload work.
    pub fn take_changed_regions(&mut self) -> impl Iterator<Item = ChangedRegion> + use<> {
        std::mem::take(&mut self.changed_regions).into_values()
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
