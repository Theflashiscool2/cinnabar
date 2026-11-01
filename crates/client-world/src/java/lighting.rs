//! Native Java local light relaxation over server-received storage. No Bedrock state IDs.
//! Rules: pinned LightEngine/BlockLightEngine/SkyLightEngine, with direct vertical sky sources
//! and native merged-face occlusion. Work is retained across frame budgets, not discarded.
use assets::registry::java::{
    fluids::{FACES, Table, optics::Optics},
    geometry::normal,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};
use world::java::{Dimension, Terrain};
type Pos = [i32; 3];
type Chunk = [i32; 2];
#[derive(Debug, Default)]
pub struct Engine {
    scope: Option<Dimension>,
    facts: Option<Arc<Table>>,
    queue: VecDeque<Pos>,
    queued: BTreeMap<Pos, u64>,
    blocked: BTreeMap<Chunk, BTreeSet<Pos>>,
    pairs: BTreeMap<(u32, u32, usize), bool>,
    pub operations: u64,
    pub changed: u64,
}
impl Engine {
    pub fn install(&mut self, facts: Arc<Table>) -> Result<(), &'static str> {
        if !facts.has_lighting() {
            return Err("Java local lighting facts missing; prepare pinned runtime assets");
        }
        self.facts = Some(facts);
        self.pairs.clear();
        Ok(())
    }
    pub fn pending(&self) -> usize {
        self.queued.len()
    }
    pub fn waiting(&self) -> usize {
        self.blocked.values().map(BTreeSet::len).sum()
    }
    fn enqueue(&mut self, t: &Terrain, p: Pos) {
        let Some(d) = t.dimension() else { return };
        if !within_light(d, p) {
            return;
        }
        let Some(g) = t.block_generation(p) else {
            self.blocked.entry(chunk(p)).or_default().insert(p);
            return;
        };
        if self.queued.insert(p, g).is_none() {
            self.queue.push_back(p);
        }
    }
    /// A new authoritative column can resume light queries, but never old piston actions.
    pub fn arrived(&mut self, t: &Terrain, key: Chunk) {
        if let Some(waiting) = self.blocked.remove(&key) {
            for p in waiting {
                self.enqueue(t, p);
            }
        }
    }
    pub fn unloaded(&mut self, key: Chunk) {
        self.queue.retain(|p| chunk(*p) != key);
        self.queued.retain(|p, _| chunk(*p) != key);
        for waiting in self.blocked.values_mut() {
            waiting.retain(|p| chunk(*p) != key);
        }
        self.blocked.retain(|_, v| !v.is_empty());
    }
    pub fn update(&mut self, t: &mut Terrain, budget: usize) -> Result<(), &'static str> {
        if self.scope.as_ref() != t.dimension() {
            self.scope = t.dimension().cloned();
            self.queue.clear();
            self.queued.clear();
            self.blocked.clear();
        }
        let Some(d) = self.scope.clone() else {
            return Ok(());
        };
        let Some(facts) = self.facts.clone() else {
            return Ok(());
        };
        for p in t.take_light_changes() {
            self.enqueue(t, p);
            for face in 0..6 {
                if let Some(q) = neighbor(p, face) {
                    self.enqueue(t, q);
                }
            }
        }
        let empty = Optics {
            dampening: 0,
            faces: Default::default(),
        };
        let mut work = BTreeMap::<Pos, [u8; 2]>::new();
        for _ in 0..budget {
            let Some(p) = self.queue.pop_front() else {
                break;
            };
            let generation = self.queued.remove(&p);
            if generation.is_none() || t.block_generation(p) != generation {
                continue;
            }
            self.operations += 1;
            let (id, emission, target) = if in_build(&d, p) {
                let id = t.block(p).ok_or("nonresident Java light target")?.0;
                let state = facts.state(id).ok_or("missing Java light state")?;
                (
                    Some(id),
                    state.emission,
                    state.light.as_ref().ok_or("missing Java light optics")?,
                )
            } else {
                (None, 0, &empty)
            };
            let old = work
                .get(&p)
                .copied()
                .or_else(|| t.light(p))
                .ok_or("invalid Java light target")?;
            let mut next = [emission, 0];
            let mut incomplete = false;
            for direction in 0..6 {
                let Some(q) = neighbor(p, direction) else {
                    continue;
                };
                let (source_id, source, levels) = if !within_light(&d, q) {
                    (
                        None,
                        &empty,
                        [0, if q[1] > p[1] && d.has_skylight { 15 } else { 0 }],
                    )
                } else {
                    if t.block_generation(q).is_none() {
                        self.blocked.entry(chunk(q)).or_default().insert(p);
                        incomplete = true;
                        continue;
                    }
                    let source_id = if in_build(&d, q) {
                        Some(t.block(q).ok_or("missing Java light neighbor")?.0)
                    } else {
                        None
                    };
                    let source = if let Some(id) = source_id {
                        facts
                            .state(id)
                            .and_then(|s| s.light.as_ref())
                            .ok_or("missing Java neighbor light optics")?
                    } else {
                        &empty
                    };
                    let levels = work
                        .get(&q)
                        .copied()
                        .or_else(|| t.light(q))
                        .ok_or("invalid Java neighbor light")?;
                    (source_id, source, levels)
                };
                let blocked = if let (Some(source_id), Some(id)) = (source_id, id) {
                    let key = (source_id, id, direction ^ 1);
                    if let Some(blocked) = self.pairs.get(&key) {
                        *blocked
                    } else {
                        let blocked = source.occludes(target, direction ^ 1);
                        // Cache capacity affects recomputation only; it never drops light work.
                        if self.pairs.len() >= 4096 {
                            self.pairs.pop_first();
                        }
                        self.pairs.insert(key, blocked);
                        blocked
                    }
                } else {
                    source.occludes(target, direction ^ 1)
                };
                if blocked {
                    continue;
                }
                let opacity = target.dampening.max(1);
                next[0] = next[0].max(levels[0].saturating_sub(opacity));
                if d.has_skylight {
                    let sky = if FACES[direction] == assets::registry::java::models::Face::Up
                        && levels[1] == 15
                        && target.dampening == 0
                    {
                        15
                    } else {
                        levels[1].saturating_sub(opacity)
                    };
                    next[1] = next[1].max(sky);
                }
            }
            if incomplete {
                continue;
            } // Unknown neighbors are never transparent air or lost sources.
            if next != old {
                work.insert(p, next);
                for face in 0..6 {
                    if let Some(q) = neighbor(p, face) {
                        self.enqueue(t, q);
                    }
                }
            }
        }
        self.changed += t.apply_local_lights(work)? as u64;
        Ok(())
    }
}
fn in_build(d: &Dimension, p: Pos) -> bool {
    i64::from(p[1]) >= i64::from(d.min_y)
        && i64::from(p[1]) < i64::from(d.min_y) + i64::from(d.height)
}
fn within_light(d: &Dimension, p: Pos) -> bool {
    i64::from(p[1]) >= i64::from(d.min_y) - 16
        && i64::from(p[1]) < i64::from(d.min_y) + i64::from(d.height) + 16
}
fn chunk(p: Pos) -> Chunk {
    [p[0].div_euclid(16), p[2].div_euclid(16)]
}
fn neighbor(p: Pos, face: usize) -> Option<Pos> {
    let step = normal(FACES[face]).to_array();
    Some([
        p[0].checked_add(step[0] as i32)?,
        p[1].checked_add(step[1] as i32)?,
        p[2].checked_add(step[2] as i32)?,
    ])
}
#[cfg(test)]
mod tests;
