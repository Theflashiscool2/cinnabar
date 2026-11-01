//! Java 26.3 PistonHeadRenderer's model identities; model-plan fixtures use the pinned registry.
pub const BASE: &str = "minecraft:piston";
pub const STICKY_BASE: &str = "minecraft:sticky_piston";
pub const HEAD: &str = "minecraft:piston_head";

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Reaction {
    PushPull,
    Push,
    Popped,
    Immoveable,
    IgnoreEntity,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sticky {
    None,
    Slime,
    Honey,
}

/// Exported state facts; visual models cannot decide whether a piston can move a block.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub reaction: Reaction,
    pub air: bool,
    pub block_entity: bool,
    pub piston_base: bool,
    pub extended: bool,
    pub unbreakable: bool,
    pub sticky: Sticky,
}

pub struct PushContext {
    pub y: i32,
    pub min_y: i32,
    /// Inclusive last buildable Y, matching Level.getMaxY in the pinned runtime.
    pub max_y: i32,
    pub within_border: bool,
    pub movement: u8,
    pub piston_direction: u8,
    pub allow_destroy: bool,
}

impl Facts {
    /// PistonBaseBlock.isPushable's predicate, without mutating blocks or assuming neighbors.
    pub fn pushable(&self, c: &PushContext) -> Result<bool, &'static str> {
        if c.movement >= 6 || c.piston_direction >= 6 || c.min_y > c.max_y {
            return Err("invalid Java piston push context");
        }
        if c.y < c.min_y || c.y > c.max_y || !c.within_border {
            return Ok(false);
        }
        if self.air {
            return Ok(true);
        }
        if (c.movement == 0 && c.y == c.min_y) || (c.movement == 1 && c.y == c.max_y) {
            return Ok(false);
        }
        if self.piston_base {
            if self.extended {
                return Ok(false);
            }
        } else {
            if self.unbreakable {
                return Ok(false);
            }
            match self.reaction {
                Reaction::Immoveable => return Ok(false),
                Reaction::Popped => return Ok(c.allow_destroy),
                Reaction::Push => return Ok(c.movement == c.piston_direction),
                Reaction::PushPull | Reaction::IgnoreEntity => {}
            }
        }
        Ok(!self.block_entity)
    }
    /// Honey and slime do not adhere to one another. The caller must still check pushability.
    pub fn sticks_to(&self, other: &Self) -> bool {
        !matches!(
            (self.sticky, other.sticky),
            (Sticky::Slime, Sticky::Honey) | (Sticky::Honey, Sticky::Slime)
        ) && (self.sticky != Sticky::None || other.sticky != Sticky::None)
    }
}

#[derive(Deserialize)]
struct Report {
    states: usize,
    max_push_depth: usize,
    runs: Vec<Run>,
}
#[derive(Deserialize)]
struct Run {
    first: u32,
    #[serde(flatten)]
    facts: Facts,
}
pub struct Table {
    states: usize,
    max_push_depth: usize,
    runs: Vec<Run>,
}
impl Table {
    /// Reads the generated, compile-time pinned report. No new per-install runtime asset.
    pub fn load(json: &str, expected_states: usize) -> Result<Self, &'static str> {
        let r: Report = serde_json::from_str(json).map_err(|_| "invalid Java piston facts")?;
        if r.states != expected_states
            || r.max_push_depth == 0
            || r.max_push_depth > r.states
            || r.runs.first().is_none_or(|r| r.first != 0)
            || r.runs.iter().any(|n| n.first as usize >= r.states)
            || r.runs.windows(2).any(|r| r[0].first >= r[1].first)
        {
            return Err("invalid Java piston facts coverage");
        }
        Ok(Self {
            states: r.states,
            max_push_depth: r.max_push_depth,
            runs: r.runs,
        })
    }
    pub fn state(&self, id: u32) -> Option<&Facts> {
        if id as usize >= self.states {
            return None;
        }
        self.runs
            .get(
                self.runs
                    .partition_point(|r| r.first <= id)
                    .checked_sub(1)?,
            )
            .map(|r| &r.facts)
    }
    pub fn max_push_depth(&self) -> usize {
        self.max_push_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_native_facts_have_no_unknown_reactions_or_coverage_holes() {
        let Ok(path) = std::env::var("CINNABAR_JAVA_PISTON_FACTS") else {
            eprintln!(
                "missing fixture: CINNABAR_JAVA_PISTON_FACTS (verified exporter piston_blocks JSON)"
            );
            return;
        };
        let text = std::fs::read_to_string(path).unwrap();
        let report: serde_json::Value = serde_json::from_str(&text).unwrap();
        let count = report["states"].as_u64().unwrap() as usize;
        let table = Table::load(&text, count).unwrap();
        for id in 0..count {
            assert!(table.state(id as u32).is_some());
        }
        assert!(table.state(count as u32).is_none());
    }
    fn stone() -> Facts {
        Facts {
            reaction: Reaction::PushPull,
            air: false,
            block_entity: false,
            piston_base: false,
            extended: false,
            unbreakable: false,
            sticky: Sticky::None,
        }
    }
    fn context() -> PushContext {
        PushContext {
            y: 37,
            min_y: -64,
            max_y: 319,
            within_border: true,
            movement: 5,
            piston_direction: 5,
            allow_destroy: false,
        }
    }
    #[test]
    fn bounds_push_reactions_and_block_entities_follow_native_predicate_order() {
        let mut state = stone();
        let mut c = context();
        assert_eq!(state.pushable(&c), Ok(true));
        state.block_entity = true;
        assert_eq!(state.pushable(&c), Ok(false));
        state.reaction = Reaction::Popped;
        c.allow_destroy = true;
        assert_eq!(state.pushable(&c), Ok(true)); // Native destroy branch precedes BE rejection.
        c.allow_destroy = false;
        assert_eq!(state.pushable(&c), Ok(false));
        state.reaction = Reaction::Push;
        assert_eq!(state.pushable(&c), Ok(true));
        c.piston_direction = 4;
        assert_eq!(state.pushable(&c), Ok(false));
        c = context();
        state = stone();
        state.unbreakable = true;
        assert_eq!(state.pushable(&c), Ok(false));
        state.piston_base = true;
        assert_eq!(state.pushable(&c), Ok(true));
        state.extended = true;
        assert_eq!(state.pushable(&c), Ok(false));
        state = stone();
        c.y = c.min_y;
        c.movement = 0;
        assert_eq!(state.pushable(&c), Ok(false));
        state.air = true;
        assert_eq!(state.pushable(&c), Ok(true));
        c.within_border = false;
        assert_eq!(state.pushable(&c), Ok(false));
        c = context();
        c.y = c.max_y;
        c.movement = 1;
        state = stone();
        assert_eq!(state.pushable(&c), Ok(false));
        c.movement = 6;
        assert!(state.pushable(&c).is_err());
    }
    #[test]
    fn slime_and_honey_branching_is_symmetric_and_not_plain_block_adhesion() {
        let mut slime = stone();
        slime.sticky = Sticky::Slime;
        let mut honey = stone();
        honey.sticky = Sticky::Honey;
        assert!(slime.sticks_to(&stone()) && stone().sticks_to(&slime));
        assert!(honey.sticks_to(&honey));
        assert!(slime.sticks_to(&slime));
        assert!(!slime.sticks_to(&honey) && !honey.sticks_to(&slime));
        assert!(!stone().sticks_to(&stone()));
    }
    #[test]
    fn run_coverage_and_unknown_reactions_never_become_normal_pushable_blocks() {
        let json = r#"{"states":3,"max_push_depth":1,"runs":[{"first":0,"reaction":"PUSH_PULL","air":true,"block_entity":false,"piston_base":false,"extended":false,"unbreakable":false,"sticky":"none"},{"first":1,"reaction":"IMMOVEABLE","air":false,"block_entity":false,"piston_base":false,"extended":false,"unbreakable":true,"sticky":"none"}]}"#;
        let t = Table::load(json, 3).unwrap();
        assert!(t.state(0).unwrap().air);
        assert_eq!(t.state(2).unwrap().reaction, Reaction::Immoveable);
        assert!(t.state(3).is_none());
        assert!(Table::load(json, 2).is_err());
        assert!(Table::load(&json.replace("\"first\":0", "\"first\":1"), 3).is_err());
        assert!(Table::load(&json.replace("\"first\":1", "\"first\":0"), 3).is_err());
        assert!(Table::load(&json.replace("IMMOVEABLE", "UNKNOWN"), 3).is_err());
    }
}
