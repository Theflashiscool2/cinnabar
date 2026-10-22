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

#[derive(Default, Debug)]
pub struct PlayerDynamics {
    pub abilities: Option<Abilities>,
    /// Only server-admitted values. Generic registry defaults are not player defaults.
    pub attributes: BTreeMap<String, Attribute>,
    /// Velocity can arrive before the first position; this does not create a position anchor.
    pub unplaced_velocity: Option<[f64; 3]>,
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
