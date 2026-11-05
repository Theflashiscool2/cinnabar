//! One native click, at most mainTarget/mainAir/offTarget/offAir. The existing controller
//! owns sequencing/backpressure; this immutable plan never predicts server mutations.
use super::{Controller, EntityTarget, Hand, Kind, Target};
use crate::java::item_use::plan::{Active, Plan, ResultKind};
use inventory::java::{Slot, Stack};
#[derive(Clone)]
pub struct UseStep {
    target: Target,
    hand: Hand,
    active: Option<(Stack, Active)>,
    terminal: bool,
}
impl UseStep {
    pub fn entity(target: EntityTarget, hand: Hand) -> Self {
        Self {
            target: Target::Entity(target),
            hand,
            active: None,
            terminal: false,
        }
    }
    pub fn block(hit: sim::java::selection::Hit, hand: Hand) -> Self {
        Self {
            target: Target::Block(hit),
            hand,
            active: None,
            terminal: false,
        }
    }
    pub fn air(
        rotation: [f32; 2],
        hand: Hand,
        plan: Plan,
        slot: &Slot,
    ) -> Result<Self, &'static str> {
        let active = match plan.result {
            ResultKind::Consume(Some(active)) => Some((
                slot.as_ref()
                    .ok_or("Native active use requires a known stack")?
                    .clone(),
                active,
            )),
            _ => None,
        };
        Ok(Self {
            target: Target::Air(rotation),
            hand,
            active,
            terminal: matches!(
                plan.result,
                ResultKind::Consume(_) | ResultKind::Unpredicted
            ),
        })
    }
    fn order(&self) -> u8 {
        (if self.hand == Hand::Off { 2 } else { 0 })
            + u8::from(matches!(self.target, Target::Air(_)))
    }
}
impl Controller {
    /// Vanilla slot selection stops MAIN use only; OFF remains attached to its
    /// own stack. Call only for a genuinely different selected slot.
    pub fn prepare_selection(&mut self, state: &client_world::java::WorldState, tick: u64) {
        self.maintain_use(state, tick, false);
        if self.using_hand() == Some(Hand::Main) {
            self.maintain_use(state, tick, true);
        }
    }
    /// Avoid resource/component planning on every rendered frame of one held click.
    pub fn use_due(&self, phase: semantic_input::ActionPhase) -> bool {
        self.enabled
            && !self.using_item()
            && self.pending.is_none()
            && (phase.pressed
                || (phase.held && self.use_elapsed >= simulation_clock::TICK_DURATION * 4))
    }

    pub fn use_chain(
        &mut self,
        state: &client_world::java::WorldState,
        phase: semantic_input::ActionPhase,
        steps: Result<Vec<UseStep>, &'static str>,
    ) {
        if self.pending.is_some() || self.using_item() {
            return;
        }
        let steps = steps.and_then(|steps| {
            if steps.len() > 4
                || steps
                    .windows(2)
                    .any(|w| w[0].order() >= w[1].order() || w[0].terminal)
            {
                Err("Invalid native hand-use ordering")
            } else {
                Ok(steps)
            }
        });
        match steps {
            Ok(steps) => {
                let mut steps = steps.into_iter();
                let first = steps.next();
                self.use_target(state, phase, Ok(first.as_ref().map(|s| s.target)));
                if self.pending.is_some()
                    && let Some(first) = first
                {
                    self.use_entity = match first.target {
                        Target::Entity(target) => Some(target),
                        _ => None,
                    };
                    self.use_revision = state.player_visual.use_revision;
                    self.bind_use_step(first);
                    self.use_steps = steps.collect();
                }
            }
            Err(e) => self.use_target(state, phase, Err(e)),
        }
    }
    /// Retire the entire unsent click, including air fallthrough, if its entity was
    /// removed or replaced while transport was full. Active use is independent.
    pub fn validate_use_entity(&mut self, state: &client_world::java::WorldState) -> bool {
        if self.use_entity.is_some_and(|target| !target.current(state)) {
            self.pending = None;
            self.use_steps.clear();
            self.prepared_use = None;
            self.use_entity = None;
            return false;
        }
        true
    }
    fn bind_use_step(&mut self, step: UseStep) {
        if let Some(request) = &mut self.pending {
            request.hand = step.hand;
        }
        self.prepared_use = step
            .active
            .map(|(stack, active)| (step.hand, stack, active));
    }
    pub(super) fn advance_use_step(&mut self, kind: Kind) {
        if !matches!(kind, Kind::UseOn | Kind::UseAir | Kind::UseEntity) || self.using_item() {
            self.use_steps.clear();
            self.use_entity = None;
            return;
        }
        if let Some(step) = self.use_steps.pop_front() {
            let kind = match step.target {
                Target::Block(_) => Kind::UseOn,
                Target::Entity(_) => Kind::UseEntity,
                Target::Air(_) => Kind::UseAir,
            };
            if self.request_target(step.target, kind) {
                self.bind_use_step(step)
            } else {
                self.use_steps.clear();
                self.prepared_use = None;
            }
        } else {
            self.use_entity = None;
        }
    }
}
