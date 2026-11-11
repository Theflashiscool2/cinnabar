//! Native Java player capabilities and attributes, independent of transport schemas.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Abilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub may_fly: bool,
    pub creative: bool,
    pub flying_speed: f32,
    pub walking_speed: f32,
}

impl Abilities {
    /// Pinned GameType.updatePlayerAbilities mutates flags in place; speeds are retained.
    pub fn apply_game_mode(&mut self, mode: super::hud::GameMode) {
        use super::hud::GameMode;
        match mode {
            GameMode::Creative => {
                self.may_fly = true;
                self.creative = true;
                self.invulnerable = true;
            }
            GameMode::Spectator => {
                self.may_fly = true;
                self.creative = false;
                self.invulnerable = true;
                self.flying = true;
            }
            GameMode::Survival | GameMode::Adventure => {
                self.may_fly = false;
                self.creative = false;
                self.invulnerable = false;
                self.flying = false;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Add,
    AddMultipliedBase,
    AddMultipliedTotal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Modifier {
    pub name: String,
    pub amount: f64,
    pub operation: Operation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub base: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub modifiers: Vec<Modifier>,
}
impl Attribute {
    /// Vanilla applies additions, scaled base additions, then total multipliers before clamping.
    /// The admitted order is retained within each group. RangedAttribute sanitization keeps
    /// arithmetic overflow within the declared bounds; malformed input is rejected separately.
    pub fn value(&self) -> Result<f64, &'static str> {
        if !self.base.is_finite()
            || self.modifiers.iter().any(|m| !m.amount.is_finite())
            || !self.minimum.is_finite()
            || !self.maximum.is_finite()
            || self.minimum > self.maximum
        {
            return Err("invalid Java attribute input");
        }
        let mut base = self.base;
        for m in &self.modifiers {
            if m.operation == Operation::Add {
                base += m.amount;
            }
        }
        let mut value = base;
        for m in &self.modifiers {
            if m.operation == Operation::AddMultipliedBase {
                value += base * m.amount;
            }
        }
        for m in &self.modifiers {
            if m.operation == Operation::AddMultipliedTotal {
                value *= 1. + m.amount;
            }
        }
        Ok(if value.is_nan() {
            self.minimum
        } else {
            value.clamp(self.minimum, self.maximum)
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Effect {
    pub amplifier: u8,
    pub duration: i32,
    pub flags: u8,
}
#[derive(Default, Debug)]
pub struct PlayerDynamics {
    pub abilities: Option<Abilities>,
    pub effects: BTreeMap<String, Effect>,
    /// Pinned player-builder values with server-authoritative replacements. Generic registry
    /// defaults are not player defaults; the admission owner supplies the verified initial set.
    pub attributes: BTreeMap<String, Attribute>,
    /// Velocity can arrive before the first position; this does not create a position anchor.
    pub unplaced_velocity: Option<[f64; 3]>,
}

impl PlayerDynamics {
    /// Effect durations advance on the native simulation cadence, never on render frames.
    pub fn tick_effects(&mut self) {
        tick_effects(&mut self.effects);
    }
}

/// Advances the same native expiry rules for a local simulation or remote visual timeline.
pub fn tick_effects(effects: &mut BTreeMap<String, Effect>) {
    effects.retain(|_, effect| {
        if effect.duration > 0 {
            effect.duration -= 1;
        }
        effect.duration != 0
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attribute_operation_order_and_clamping_are_authoritative() {
        let mut a = Attribute {
            base: 10.,
            minimum: 0.,
            maximum: 100.,
            modifiers: vec![
                Modifier {
                    name: "total".into(),
                    amount: 0.5,
                    operation: Operation::AddMultipliedTotal,
                },
                Modifier {
                    name: "base".into(),
                    amount: 0.25,
                    operation: Operation::AddMultipliedBase,
                },
                Modifier {
                    name: "add".into(),
                    amount: 2.,
                    operation: Operation::Add,
                },
            ],
        };
        assert_eq!(a.value(), Ok(22.5));
        a.maximum = 20.;
        assert_eq!(a.value(), Ok(20.));
        a.modifiers[0].amount = f64::MAX;
        assert_eq!(a.value(), Ok(20.));
        a.modifiers.push(Modifier {
            name: "zero".into(),
            amount: -1.,
            operation: Operation::AddMultipliedTotal,
        });
        assert_eq!(a.value(), Ok(0.)); // Positive infinity multiplied by zero is sanitized NaN.
        a.base = f64::NAN;
        assert!(a.value().is_err());
    }
}

#[cfg(test)]
mod ability_mode_tests {
    use super::*;
    use crate::java::hud::GameMode;

    #[test]
    fn native_game_mode_updates_flags_in_place_and_retains_speeds() {
        let rows: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java-ability-modes.json"))
                .unwrap();
        for row in rows.as_array().unwrap() {
            let flags = row["initial_flags"].as_u64().unwrap();
            let mut abilities = Abilities {
                invulnerable: flags & 1 != 0,
                flying: flags & 2 != 0,
                may_fly: flags & 4 != 0,
                creative: flags & 8 != 0,
                flying_speed: -0.01337,
                walking_speed: 0.123,
            };
            let mode = match row["mode"].as_u64().unwrap() {
                0 => GameMode::Survival,
                1 => GameMode::Creative,
                2 => GameMode::Adventure,
                3 => GameMode::Spectator,
                id => panic!("unknown native mode {id}"),
            };
            abilities.apply_game_mode(mode);
            let flags = u8::from(abilities.invulnerable)
                | u8::from(abilities.flying) << 1
                | u8::from(abilities.may_fly) << 2
                | u8::from(abilities.creative) << 3;
            assert_eq!(u64::from(flags), row["flags"].as_u64().unwrap(), "{row}");
            assert_eq!(
                abilities.flying_speed,
                row["flying_speed"].as_f64().unwrap() as f32
            );
            assert_eq!(
                abilities.walking_speed,
                row["walking_speed"].as_f64().unwrap() as f32
            );
        }
    }
}
