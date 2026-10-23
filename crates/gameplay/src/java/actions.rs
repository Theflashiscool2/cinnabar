//! Orders hotbar choices and attack edges across bounded transport backpressure.
use client_world::java::WorldState;
use inventory::java::HOTBAR_SIZE;
use semantic_input::{
    Action, ActionPhase, HOTBAR_DIGIT_ACTIONS,
    finalized::FinalizedInput,
    identity::{InputAuthorityGeneration, InputSequence},
};
use sim::java::selection::Hit;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Slot(usize),
    Cycle(i32),
    Attack(bool),
    Use(bool),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub epoch: u64,
    pub sequence: u64,
    pub slot: u8,
}
#[derive(Clone, Copy)]
struct Entry {
    intent: Intent,
    hit: Option<Hit>,
}
#[derive(Default)]
pub struct Controller {
    scope: Option<(u64, u64)>,
    authority: Option<InputAuthorityGeneration>,
    frame: Option<InputSequence>,
    correction: u64,
    queue: VecDeque<Entry>,
    pending: Option<Selection>,
    waiting: Option<u64>,
    next_sequence: u64,
    held: bool,
    use_held: bool,
    quarantined: bool,
    pub blocked: Option<&'static str>,
}
impl Controller {
    /// Captures a finalized frame once. Authority changes retire only unsent intent;
    /// write receipts remain ordered with server selection corrections in the inventory owner.
    pub fn capture(&mut self, state: &WorldState, input: &FinalizedInput, hit: Option<Hit>) {
        let scope = state.terrain.dimension().map(|d| (d.session, d.epoch));
        if self.scope != scope {
            *self = Self {
                scope,
                frame: self.frame,
                ..Default::default()
            };
        }
        let snapshot = input.snapshot();
        let authority = snapshot.map(|s| s.authority_generation);
        if self.authority != authority
            || self.correction != state.motion_sequence()
            || snapshot.is_none_or(|s| s.authority_released)
            || state.awaiting_correction_ack()
            || state.death.is_some()
        {
            self.retire();
        }
        self.authority = authority;
        self.correction = state.motion_sequence();
        let Some(snapshot) = snapshot else { return };
        if self.frame == Some(snapshot.frame_sequence) {
            return;
        }
        self.frame = Some(snapshot.frame_sequence);
        if scope.is_none()
            || snapshot.authority_released
            || state.awaiting_correction_ack()
            || state.death.is_some()
            || state.vitals.as_ref().is_none_or(|v| v.health <= 0.)
        {
            return;
        }
        let neutral = snapshot
            .ordered
            .as_ref()
            .map_or_else(|| snapshot.phases.iter().all(|p| !p.held), |o| o.neutral());
        if self.quarantined {
            if neutral {
                self.quarantined = false;
                self.blocked = None;
            }
            return;
        }
        if let Some(ordered) = &snapshot.ordered {
            if ordered.overflowed() {
                self.overflow();
                return;
            }
            self.append(
                ordered
                    .events()
                    .iter()
                    .filter_map(|e| intent(e.action, e.pressed)),
                hit,
            );
        } else {
            // Controller/touch frames have no desktop event order; choose selection before attack.
            let choices = HOTBAR_DIGIT_ACTIONS
                .iter()
                .copied()
                .chain([Action::HotbarPrevious, Action::HotbarNext])
                .filter(|&a| input.phase(a).pressed)
                .filter_map(|a| intent(a, true));
            self.append(choices, hit);
            let attack = input.phase(Action::Attack);
            if attack.pressed {
                self.append([Intent::Attack(true)], hit);
            }
            if attack.released {
                self.append([Intent::Attack(false)], hit);
            }
            let use_item = input.phase(Action::Use);
            if use_item.pressed {
                self.append([Intent::Use(true)], hit);
            }
            if use_item.released {
                self.append([Intent::Use(false)], hit);
            }
        }
    }
    fn retire(&mut self) {
        self.queue.clear();
        self.pending = None;
        self.waiting = None;
        self.held = false;
        self.use_held = false;
    }
    fn overflow(&mut self) {
        self.retire();
        self.quarantined = true;
        self.blocked = Some("Java input queue overflow; release controls before retrying");
    }
    fn append(&mut self, values: impl IntoIterator<Item = Intent>, hit: Option<Hit>) {
        for value in values {
            if self.quarantined {
                break;
            }
            self.queue.push_back(Entry { intent: value, hit });
        }
    }
    /// No later action can pass a selection until its ordered socket receipt is admitted.
    pub fn front(&mut self, state: &WorldState) -> Option<Intent> {
        if let Some(sequence) = self.waiting {
            if state
                .inventory
                .selection_receipt
                .is_none_or(|r| r < sequence)
            {
                return None;
            }
            self.waiting = None;
        }
        self.queue.front().map(|entry| entry.intent)
    }
    /// Returns the click-time target, rather than repicking after a delayed selection write.
    pub fn consume_attack(&mut self, pressed: bool) -> (ActionPhase, Option<Hit>) {
        let consumed = self.queue.pop_front().expect("observed attack edge");
        debug_assert_eq!(consumed.intent, Intent::Attack(pressed));
        self.held = pressed;
        (
            ActionPhase {
                pressed,
                held: pressed,
                released: !pressed,
            },
            consumed.hit,
        )
    }
    /// Use edges obey the same selection and click-time target ordering as attacks.
    pub fn consume_use(&mut self, pressed: bool) -> (ActionPhase, Option<Hit>) {
        let consumed = self.queue.pop_front().expect("observed use edge");
        debug_assert_eq!(consumed.intent, Intent::Use(pressed));
        self.use_held = pressed;
        (
            ActionPhase {
                pressed,
                held: pressed,
                released: !pressed,
            },
            consumed.hit,
        )
    }
    /// Computes against admitted selection, including server corrections between writes.
    pub fn selection(&mut self, state: &WorldState) -> Option<Selection> {
        if self.pending.is_some() {
            return self.pending;
        }
        let slot = match self.front(state)? {
            Intent::Slot(slot) => slot,
            Intent::Cycle(delta) => {
                (state.inventory.selected? as i32 + delta).rem_euclid(HOTBAR_SIZE as i32) as usize
            }
            Intent::Attack(_) | Intent::Use(_) => return None,
        };
        if slot >= HOTBAR_SIZE {
            self.overflow();
            return None;
        }
        if state.inventory.selected == Some(slot) {
            self.queue.pop_front();
            return None;
        }
        let Some(sequence) = self.next_sequence.checked_add(1) else {
            self.blocked = Some("Java selection sequence exhausted");
            return None;
        };
        self.pending = Some(Selection {
            epoch: self.scope?.1,
            sequence,
            slot: slot as u8,
        });
        self.pending
    }
    /// Transport-full retries keep the same choice and receipt; no stack is locally mutated.
    pub fn selection_queued(&mut self) {
        if let Some(selection) = self.pending.take() {
            self.queue.pop_front();
            self.next_sequence = selection.sequence;
            self.waiting = Some(selection.sequence);
        }
    }
    pub fn held_attack(&self) -> ActionPhase {
        ActionPhase {
            held: self.held && self.queue.is_empty() && self.waiting.is_none(),
            ..Default::default()
        }
    }
    pub fn held_use(&self) -> ActionPhase {
        ActionPhase {
            held: self.use_held && self.queue.is_empty() && self.waiting.is_none(),
            ..Default::default()
        }
    }
    pub fn backlog(&self) -> usize {
        self.queue.len() + usize::from(self.waiting.is_some())
    }
}
fn intent(action: Action, pressed: bool) -> Option<Intent> {
    if action == Action::Use {
        return Some(Intent::Use(pressed));
    }
    if action == Action::Attack {
        return Some(Intent::Attack(pressed));
    }
    if !pressed {
        return None;
    }
    if let Some(slot) = HOTBAR_DIGIT_ACTIONS.iter().position(|&a| a == action) {
        return Some(Intent::Slot(slot));
    }
    match action {
        Action::HotbarPrevious => Some(Intent::Cycle(-1)),
        Action::HotbarNext => Some(Intent::Cycle(1)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
