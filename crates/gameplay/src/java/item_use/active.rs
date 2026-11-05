//! Local native-use lifetime. Only accepted UseItem requests start it; inventory and
//! consumable results remain authoritative. Duration drives animation, never completion.
use super::plan::{Active, Effects, Hand};
use inventory::java::{Slot, Stack};

pub struct Using {
    hand: Hand,
    item: Stack,
    plan: Active,
    started: u64,
}
impl Using {
    pub fn start(item: Stack, plan: Active, tick: u64) -> Self {
        Self::start_in(Hand::Main, item, plan, tick)
    }
    pub fn start_in(hand: Hand, item: Stack, plan: Active, tick: u64) -> Self {
        Self {
            hand,
            item,
            plan,
            started: tick,
        }
    }
    pub fn hand(&self) -> Hand {
        self.hand
    }
    pub fn matches(&self, held: Option<&Slot>) -> bool {
        held.and_then(Option::as_ref).is_some_and(|item| {
            // LivingEntity.updatingUsingItem uses isSameItem, not component or
            // count equality. Normal shield durability updates must not restart use.
            item.item == self.item.item
        })
    }
    pub fn elapsed(&self, tick: u64) -> u32 {
        tick.saturating_sub(self.started).min(u64::from(u32::MAX)) as u32
    }
    pub fn duration(&self) -> u32 {
        self.plan.ticks
    }
    pub fn effects(&self) -> Effects {
        self.plan.effects
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn use_lifetime_ignores_counts_but_retires_replacement_and_uses_native_ticks() {
        let item = Stack {
            item: "minecraft:apple".into(),
            count: 3,
            added: Default::default(),
            removed: Default::default(),
        };
        let plan = Active {
            ticks: 32,
            end: super::super::plan::End::Complete,
            effects: Effects {
                can_sprint: false,
                speed_multiplier: 0.2,
            },
        };
        let using = Using::start(item.clone(), plan, 10);
        let mut item = item;
        item.count = 2;
        assert!(using.matches(Some(&Some(item.clone()))));
        assert_eq!(using.elapsed(41), 31);
        assert_eq!(using.elapsed(42), 32);
        item.added
            .insert("minecraft:damage".into(), serde_json::json!(4));
        assert!(
            using.matches(Some(&Some(item.clone()))),
            "native same-item wear does not stop active use"
        );
        item.item = "minecraft:bread".into();
        assert!(!using.matches(Some(&Some(item))));
        assert!(!using.matches(Some(&None)));
        assert!(!using.matches(None));
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;
    #[test]
    fn charged_use_waits_for_release_even_after_nominal_duration() {
        let item = Stack {
            item: "minecraft:bow".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        };
        let plan = Active {
            ticks: 72000,
            end: super::super::plan::End::Release,
            effects: Effects {
                can_sprint: false,
                speed_multiplier: 0.2,
            },
        };
        let using = Using::start(item, plan, 0);
        assert_eq!(using.elapsed(72001), 72001);
    }
}
