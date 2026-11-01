//! Applies server-triggered piston motion before subsequent terrain packets are coalesced.
use assets::registry::java::piston::{Kind, Reaction, Table};
use sim::java::piston::structure::{self, Access, Error};
use std::{cell::RefCell, collections::BTreeMap};
use world::java::{
    BlockStateId, Terrain,
    piston::{Motion, Moving, step},
};
#[cfg(test)]
mod tests;

struct View<'a> {
    terrain: &'a Terrain,
    table: &'a Table,
    omit: Option<[i32; 3]>,
    finished: Vec<[i32; 3]>,
    reads: RefCell<BTreeMap<[i32; 3], Option<u64>>>,
}
impl Access for View<'_> {
    fn block(&self, p: [i32; 3]) -> Result<assets::registry::java::piston::Facts, Error> {
        let d = self.terrain.dimension().ok_or(Error::InvalidContext)?;
        if p[1] >= d.min_y && p[1] < d.min_y + d.height {
            self.reads
                .borrow_mut()
                .insert(p, self.terrain.block_generation(p));
        }
        let id = if Some(p) == self.omit || p[1] < d.min_y || p[1] >= d.min_y + d.height {
            self.table.air
        } else {
            if self.finished.contains(&p) {
                self.terrain.pistons.get(&p).map(|m| m.state)
            } else {
                None
            }
            .or_else(|| self.terrain.block(p).map(|id| id.0))
            .ok_or(Error::MissingData(p))?
        };
        self.table.state(id).copied().ok_or(Error::MissingData(p))
    }
    fn height(&self) -> (i32, i32) {
        let d = self.terrain.dimension().unwrap();
        (d.min_y, d.min_y + d.height - 1)
    }
    // The event is server-authorized. Custom-border reconstruction is separate from storage.
    fn within_border(&self, _: [i32; 3]) -> bool {
        true
    }
    fn max_push_depth(&self) -> usize {
        self.table.max_push_depth()
    }
}
fn offset(p: [i32; 3], direction: u8, n: i32) -> Result<[i32; 3], &'static str> {
    let v = step(direction).ok_or("invalid Java piston direction")?;
    let mut q = p;
    for i in 0..3 {
        q[i] = p[i]
            .checked_add(v[i] * n)
            .ok_or("Java piston coordinate overflow")?;
    }
    Ok(q)
}
fn start(
    t: &mut Terrain,
    p: [i32; 3],
    host: u32,
    state: u32,
    facing: u8,
    extending: bool,
    source: bool,
) -> Result<(), &'static str> {
    t.set_block(p, BlockStateId(host));
    let generation = t
        .block_generation(p)
        .ok_or("Java piston target not resident")?;
    t.pistons.insert(
        p,
        Moving {
            state,
            facing,
            extending,
            source,
            motion: Motion::new(0.)?,
            generation,
        },
    );
    Ok(())
}
fn finish(t: &mut Terrain, p: [i32; 3]) {
    if let Some(m) = t.pistons.remove(&p) {
        t.set_block(p, BlockStateId(m.state));
    }
}
pub(crate) fn event(
    t: &mut Terrain,
    p: [i32; 3],
    block: u32,
    action: u8,
    value: u8,
) -> Result<bool, &'static str> {
    let table = super::facts()?;
    t.pending_pistons.remove(&p);
    let Some(id) = t.block(p) else {
        return Ok(false);
    };
    let Some(base) = table
        .variant(id.0)
        .filter(|v| v.kind == Kind::Base && v.block == block)
        .copied()
    else {
        return Ok(false);
    };
    if action > 2 {
        return Ok(false);
    }
    let facing = base.facing;
    let extending = action == 0;
    let front = offset(p, facing, 1)?;
    let moving = table
        .select(Kind::Moving, facing, base.sticky, false, false)
        .ok_or("missing native moving-piston variant")?;
    // Resolve against virtual completion first. An incomplete neighbor view must not partly
    // retire motion or erase a block before the plan is known.
    let mut finished = Vec::new();
    let far = offset(p, facing, 2)?;
    let pull = extending
        || !t
            .pistons
            .get(&far)
            .is_some_and(|m| m.facing == facing && m.extending);
    if !extending {
        finished.push(front);
        if !pull {
            finished.push(far);
        }
    }
    let access = View {
        terrain: t,
        table,
        omit: (!extending).then_some(front),
        finished,
        reads: RefCell::new(BTreeMap::from([(p, t.block_generation(p))])),
    };
    let resolve = (|| {
        if extending || (base.sticky && action == 1 && pull) {
            let eligible = if extending {
                true
            } else {
                let state = access.block(far)?;
                state.reaction == Reaction::PushPull || state.piston_base
            };
            if eligible {
                structure::resolve(&access, p, facing, extending)
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    })();
    let resolution = match resolve {
        Ok(r) => r,
        Err(Error::MissingData(_)) => {
            let dependencies = access.reads.into_inner();
            t.pending_pistons
                .insert(p, world::java::piston::Pending { dependencies });
            return Ok(false);
        }
        Err(Error::InvalidContext) => return Err("invalid Java piston structure context"),
    };
    if extending && resolution.is_none() {
        return Ok(false);
    }
    let sources = resolution
        .as_ref()
        .map(|r| {
            r.push
                .iter()
                .map(|p| {
                    Ok((
                        *p,
                        if access.finished.contains(p) {
                            t.pistons
                                .get(p)
                                .map(|m| m.state)
                                .or_else(|| t.block(*p).map(|id| id.0))
                        } else {
                            t.block(*p).map(|id| id.0)
                        }
                        .ok_or("Java piston source disappeared")?,
                    ))
                })
                .collect::<Result<Vec<_>, &'static str>>()
        })
        .transpose()?
        .unwrap_or_default();
    let finished = access.finished;
    for q in finished {
        finish(t, q);
    }
    t.pending_pistons.remove(&p);
    if let Some(r) = &resolution {
        for q in &r.destroy {
            t.set_block(*q, BlockStateId(table.air));
        }
        for (q, _) in &sources {
            t.set_block(*q, BlockStateId(table.air));
        }
    }
    if !extending {
        t.set_block(front, BlockStateId(table.air));
    }
    for (q, state) in sources.into_iter().rev() {
        start(
            t,
            offset(q, if extending { facing } else { facing ^ 1 }, 1)?,
            moving,
            state,
            facing,
            extending,
            false,
        )?;
    }
    if extending {
        let head = table
            .select(Kind::Head, facing, base.sticky, false, false)
            .ok_or("missing native piston head")?;
        start(t, front, moving, head, facing, true, true)?;
        t.set_block(
            p,
            BlockStateId(
                table
                    .select(Kind::Base, facing, base.sticky, true, false)
                    .ok_or("missing extended piston base")?,
            ),
        );
    } else {
        let moved = table
            .select(Kind::Base, (value & 7) % 6, base.sticky, false, false)
            .ok_or("missing retracting piston base")?;
        start(t, p, moving, moved, facing, false, true)?;
    }
    Ok(true)
}
