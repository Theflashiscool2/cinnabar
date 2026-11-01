//! Bounded native Java mesh workers. A result never outlives its session/dimension authority.
use assets::registry::java::graphics::{Extension, Graphics};
use crossbeam_channel::{Receiver, Sender};
use meshing::{
    ChunkMesh, PackedBiomeRecord,
    java::{Error, Snapshot},
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use world::java::{Dimension, PublicationKey, Terrain};

/// Work/output bound, not a content cap: queued resident sections wait for a worker slot.
const WORKERS: usize = 4;
type SectionKey = [i32; 3];
struct Completed {
    key: PublicationKey,
    section: SectionKey,
    result: Result<(ChunkMesh, PackedBiomeRecord), Error>,
}
pub struct Ready {
    pub key: PublicationKey,
    pub section: SectionKey,
    pub mesh: ChunkMesh,
    pub biome: PackedBiomeRecord,
}
#[derive(Default, Debug)]
pub struct Metrics {
    pub dispatched: u64,
    pub completed: u64,
    pub obsolete: u64,
    pub waiting_light: u64,
    pub waiting_biome: u64,
}
/// Mesh jobs capture immutable graphics. Missing states wait for a checked append-only extension;
/// existing results remain valid because their resource addresses cannot change.
pub struct Stream {
    dimension: Dimension,
    graphics: Arc<Graphics>,
    focus: SectionKey,
    pending: BTreeSet<SectionKey>,
    active: BTreeSet<SectionKey>,
    ready: VecDeque<Ready>,
    errors: BTreeMap<SectionKey, String>,
    waiting_graphics: BTreeMap<SectionKey, BTreeSet<u32>>,
    sender: Sender<Completed>,
    receiver: Receiver<Completed>,
    cancel: Arc<AtomicBool>,
    pub metrics: Metrics,
}
impl std::fmt::Debug for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JavaMeshStream")
            .field("dimension", &self.dimension)
            .field("pending", &self.pending.len())
            .field("active", &self.active.len())
            .field("ready", &self.ready.len())
            .field("metrics", &self.metrics)
            .finish()
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Stream {
    pub fn new(terrain: &Terrain, graphics: Arc<Graphics>) -> Option<Self> {
        let dimension = terrain.dimension()?.clone();
        let center = terrain.center.unwrap_or_default();
        let focus = [center[0], dimension.min_y.div_euclid(16), center[1]];
        let (sender, receiver) = crossbeam_channel::bounded(WORKERS);
        let mut stream = Self {
            dimension,
            graphics,
            focus,
            pending: BTreeSet::new(),
            active: BTreeSet::new(),
            ready: VecDeque::new(),
            errors: BTreeMap::new(),
            waiting_graphics: BTreeMap::new(),
            sender,
            receiver,
            cancel: Arc::new(AtomicBool::new(false)),
            metrics: Metrics::default(),
        };
        for (chunk, _) in terrain.columns() {
            stream.mark(chunk);
        }
        Some(stream)
    }
    pub fn graphics(&self) -> &Arc<Graphics> {
        &self.graphics
    }
    /// Installs only a proven extension of this stream's current catalog in this world lifetime.
    pub fn install_graphics(&mut self, terrain: &Terrain, extension: &Extension) -> bool {
        if terrain.dimension() != Some(&self.dimension) || !extension.extends(&self.graphics) {
            return false;
        }
        self.graphics = Arc::clone(extension.graphics());
        self.waiting_graphics.retain(|section, ids| {
            if ids.iter().all(|id| self.graphics.contains(*id)) {
                self.pending.insert(*section);
                false
            } else {
                true
            }
        });
        true
    }
    pub fn missing_graphics(&self) -> BTreeSet<u32> {
        self.waiting_graphics
            .values()
            .flatten()
            .copied()
            .filter(|id| !self.graphics.contains(*id))
            .collect()
    }
    pub fn dimension(&self) -> &Dimension {
        &self.dimension
    }
    /// Prioritizes pending work near the current view without replacing its authority or
    /// discarding distant sections. Existing worker/result lifetime checks remain unchanged.
    pub fn set_focus(&mut self, section: [i32; 3]) {
        self.focus = section;
    }
    fn mark(&mut self, [x, z]: [i32; 2]) {
        for y in self.dimension.min_y / 16..(self.dimension.min_y + self.dimension.height) / 16 {
            self.pending.insert([x, y, z]);
        }
    }
    /// Column revisions conservatively fence every job in the halo. A spatial edit can retire
    /// an unrelated in-flight result; retry that bounded job rather than losing its first mesh.
    fn retry_obsolete(&mut self, terrain: &Terrain, section: SectionKey) {
        self.metrics.obsolete += 1;
        if terrain.dimension() == Some(&self.dimension)
            && terrain.column([section[0], section[2]]).is_some()
        {
            self.pending.insert(section);
        }
    }
    /// Returns unloaded columns so the host can retire already published render entities.
    /// Every dispatch captures immutable authority, and every result is checked again at handoff.
    pub fn poll(&mut self, terrain: &mut Terrain) -> Vec<[i32; 2]> {
        if terrain.dimension() != Some(&self.dimension) {
            self.cancel.store(true, Ordering::Release);
            self.pending.clear();
            self.waiting_graphics.clear();
            self.ready.clear();
            return Vec::new();
        }
        let dirty = terrain.take_dirty();
        let mut unloaded = Vec::new();
        for chunk in dirty {
            self.errors.retain(|p, _| [p[0], p[2]] != chunk);
            self.waiting_graphics.retain(|p, _| [p[0], p[2]] != chunk);
            if terrain.column(chunk).is_none() {
                unloaded.push(chunk);
                self.pending.retain(|p| [p[0], p[2]] != chunk);
            } else {
                self.mark(chunk);
            }
        }
        for region in terrain.take_changed_regions() {
            for section in meshing::java::affected_sections(region) {
                if section[1] < self.dimension.min_y.div_euclid(16)
                    || section[1] >= (self.dimension.min_y + self.dimension.height).div_euclid(16)
                    || terrain.column([section[0], section[2]]).is_none()
                {
                    continue;
                }
                self.errors.remove(&section);
                self.waiting_graphics.remove(&section);
                self.pending.insert(section);
            }
        }
        while self.ready.len() < WORKERS {
            let Ok(done) = self.receiver.try_recv() else {
                break;
            };
            self.active.remove(&done.section);
            if !terrain.is_current(&done.key) {
                self.retry_obsolete(terrain, done.section);
                continue;
            }
            self.metrics.completed += 1;
            match done.result {
                Ok((mesh, biome)) => {
                    self.errors.remove(&done.section);
                    self.ready.push_back(Ready {
                        key: done.key,
                        section: done.section,
                        mesh,
                        biome,
                    });
                }
                Err(Error::MissingGraphics(ids)) => {
                    if ids.iter().all(|id| self.graphics.contains(*id)) {
                        // An old worker can finish after its requested assets were installed.
                        self.pending.insert(done.section);
                    } else {
                        self.waiting_graphics.insert(done.section, ids);
                    }
                }
                Err(Error::MissingBiome(_)) => {
                    self.metrics.waiting_biome += 1;
                }
                Err(Error::MissingLight(_)) => {
                    self.metrics.waiting_light += 1;
                }
                Err(error) => {
                    self.errors.insert(done.section, error.to_string());
                }
            }
        }
        // The queue is worker-bounded. Preserve ordering while retiring stale candidates.
        for _ in 0..self.ready.len() {
            let ready = self.ready.pop_front().expect("counted ready result");
            if terrain.is_current(&ready.key) {
                self.ready.push_back(ready);
            } else {
                self.retry_obsolete(terrain, ready.section);
            }
        }
        while self.active.len() + self.ready.len() < WORKERS {
            let Some(section) = self
                .pending
                .iter()
                .filter(|p| !self.active.contains(*p))
                .min_by_key(|p| {
                    let distance = (0..3)
                        .map(|i| u128::from(p[i].abs_diff(self.focus[i])).pow(2))
                        .sum::<u128>();
                    (distance, **p)
                })
                .copied()
            else {
                break;
            };
            self.pending.remove(&section);
            let Some(snapshot) = Snapshot::capture(terrain, [section[0], section[2]]) else {
                continue;
            };
            let graphics = Arc::clone(&self.graphics);
            let sender = self.sender.clone();
            let cancel = Arc::clone(&self.cancel);
            self.active.insert(section);
            self.metrics.dispatched += 1;
            crate::stream::workers::WORKERS.spawn(crate::stream::workers::Lane::Mesh, move || {
                if cancel.load(Ordering::Acquire) {
                    return;
                }
                let result = section_biome(&snapshot, section[1]).and_then(|biome| {
                    meshing::java::mesh(&snapshot, section[1], &graphics).map(|mesh| (mesh, biome))
                });
                if cancel.load(Ordering::Acquire) {
                    return;
                }
                // One result per occupied slot; capacity equals the dispatch bound. No event drops.
                let _ = sender.send(Completed {
                    key: snapshot.key,
                    section,
                    result,
                });
            });
        }
        unloaded
    }
    pub fn pop_ready(&mut self, terrain: &Terrain) -> Option<Ready> {
        while let Some(ready) = self.ready.pop_front() {
            if terrain.is_current(&ready.key) {
                return Some(ready);
            }
            self.retry_obsolete(terrain, ready.section);
        }
        None
    }
    pub fn retry_front(&mut self, ready: Ready) {
        self.ready.push_front(ready);
    }
    pub fn errors(&self) -> &BTreeMap<SectionKey, String> {
        &self.errors
    }
    pub fn backlog(&self) -> usize {
        self.pending.len() + self.active.len() + self.ready.len() + self.waiting_graphics.len()
    }
}
/// Uniform fields use a compact descriptor; mixed fields use native seeded sampling.
fn section_biome(snapshot: &Snapshot, section_y: i32) -> Result<PackedBiomeRecord, Error> {
    let mut uniform = None;
    for column in snapshot.columns.values() {
        for dy in -1..=1 {
            let section = (section_y + dy - snapshot.dimension.min_y / 16)
                .clamp(0, column.sections.len() as i32 - 1) as usize;
            let biomes = &column.sections[section].biomes;
            for i in 0..biomes.len() {
                let id = biomes
                    .get(i)
                    .ok_or_else(|| Error::Invalid("missing Java biome value".into()))?
                    .0;
                if uniform.is_some_and(|old| old != id) {
                    return meshing::java::biomes::record(snapshot, section_y);
                }
                uniform = Some(id);
            }
        }
    }
    PackedBiomeRecord::uniform(
        uniform.ok_or_else(|| Error::Invalid("missing Java biome field".into()))?,
    )
    .ok_or_else(|| Error::Invalid("Java tint index exceeds descriptor".into()))
}

#[cfg(test)]
mod tests;
