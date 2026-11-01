//! Read-only native piston push/pull planning. Applying an event is a separate authority step.
//! Reference: Java 26.3 PistonStructureResolver; output order matters for overlapping chains.
use assets::registry::java::piston::{Facts, PushContext, Reaction, Sticky};
use world::java::piston::step;
#[cfg(test)]
mod tests;

pub type Position = [i32; 3];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    MissingData(Position),
    InvalidContext,
}
pub trait Access {
    /// Missing resident state/facts must defer resolution, never stand in for air.
    fn block(&self, position: Position) -> Result<Facts, Error>;
    fn within_border(&self, position: Position) -> bool;
    fn height(&self) -> (i32, i32);
    /// Limit from the pinned native facts, not a server-selected allocation budget.
    fn max_push_depth(&self) -> usize;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolution {
    pub push: Vec<Position>,
    pub destroy: Vec<Position>,
}

/// `None` means the structure cannot move. No blocks, inventory or world generations change.
pub fn resolve(
    world: &impl Access,
    piston: Position,
    facing: u8,
    extending: bool,
) -> Result<Option<Resolution>, Error> {
    let movement = if extending { facing } else { facing ^ 1 };
    if facing >= 6 {
        return Err(Error::InvalidContext);
    }
    let start = shifted(piston, facing, if extending { 1 } else { 2 })?;
    let state = world.block(start)?;
    let mut r = Resolver {
        world,
        piston,
        movement,
        result: Resolution::default(),
    };
    if !r.pushable(state, start, false, facing)? {
        if extending && state.reaction == Reaction::Popped {
            r.result.destroy.push(start);
            return Ok(Some(r.result));
        }
        return Ok(None);
    }
    if !r.line(start, movement)? {
        return Ok(None);
    }
    let mut index = 0;
    while index < r.result.push.len() {
        let p = r.result.push[index];
        if world.block(p)?.sticky != Sticky::None && !r.branch(p)? {
            return Ok(None);
        }
        index += 1;
    }
    Ok(Some(r.result))
}

struct Resolver<'a, W> {
    world: &'a W,
    piston: Position,
    movement: u8,
    result: Resolution,
}
impl<W: Access> Resolver<'_, W> {
    fn pushable(
        &self,
        state: Facts,
        p: Position,
        destroy: bool,
        direction: u8,
    ) -> Result<bool, Error> {
        let (min_y, max_y) = self.world.height();
        state
            .pushable(&PushContext {
                y: p[1],
                min_y,
                max_y,
                within_border: self.world.within_border(p),
                movement: self.movement,
                piston_direction: direction,
                allow_destroy: destroy,
            })
            .map_err(|_| Error::InvalidContext)
    }
    fn line(&mut self, p: Position, from: u8) -> Result<bool, Error> {
        let mut state = self.world.block(p)?;
        if state.air
            || !self.pushable(state, p, false, from)?
            || p == self.piston
            || self.result.push.contains(&p)
        {
            return Ok(true);
        }
        let mut behind = 1;
        if self.result.push.len() + behind > self.world.max_push_depth() {
            return Ok(false);
        }
        while state.sticky != Sticky::None {
            let q = shifted(p, self.movement ^ 1, behind as i32)?;
            let previous = state;
            state = self.world.block(q)?;
            if state.air
                || !previous.sticks_to(&state)
                || !self.pushable(state, q, false, self.movement ^ 1)?
                || q == self.piston
            {
                break;
            }
            behind += 1;
            if self.result.push.len() + behind > self.world.max_push_depth() {
                return Ok(false);
            }
        }
        for distance in (0..behind).rev() {
            self.result
                .push
                .push(shifted(p, self.movement ^ 1, distance as i32)?);
        }
        let mut added = behind;
        let mut ahead = 1;
        loop {
            let q = shifted(p, self.movement, ahead)?;
            if let Some(collision) = self.result.push.iter().position(|p| *p == q) {
                let tail = self.result.push.len() - added;
                self.result.push[collision..].rotate_left(tail - collision);
                // Native checks through the collision plus the newly moved tail, inclusively.
                for index in 0..=collision + added {
                    let p = self.result.push[index];
                    if self.world.block(p)?.sticky != Sticky::None && !self.branch(p)? {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
            state = self.world.block(q)?;
            if state.air {
                return Ok(true);
            }
            if !self.pushable(state, q, true, self.movement)? || q == self.piston {
                return Ok(false);
            }
            if state.reaction == Reaction::Popped {
                self.result.destroy.push(q);
                return Ok(true);
            }
            if self.result.push.len() >= self.world.max_push_depth() {
                return Ok(false);
            }
            self.result.push.push(q);
            added += 1;
            ahead += 1;
        }
    }
    fn branch(&mut self, p: Position) -> Result<bool, Error> {
        let state = self.world.block(p)?;
        for direction in 0..6 {
            if direction / 2 == self.movement / 2 {
                continue;
            }
            let q = shifted(p, direction, 1)?;
            if self.world.block(q)?.sticks_to(&state) && !self.line(q, direction)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
fn shifted(p: Position, direction: u8, distance: i32) -> Result<Position, Error> {
    let vector = step(direction).ok_or(Error::InvalidContext)?;
    let mut q = p;
    for i in 0..3 {
        q[i] = p[i]
            .checked_add(
                vector[i]
                    .checked_mul(distance)
                    .ok_or(Error::InvalidContext)?,
            )
            .ok_or(Error::InvalidContext)?;
    }
    Ok(q)
}
