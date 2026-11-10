//! Native use-result planning. Results choose packets/presentation, never award an
//! effect, damage, equipment or consumption; those remain server-owned facts.
use super::Defaults;
use assets::registry::java::items::defaults;
use inventory::java::{Slot, Stack, cooldown::Cooldowns};
use serde_json::Value;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hand {
    #[default]
    Main,
    Off,
}
impl Hand {
    /// Unknown and known-empty remain distinct for either authoritative hand.
    pub fn held(self, inventory: &inventory::java::State) -> Option<&Slot> {
        match self {
            Self::Main => inventory.held(),
            Self::Off => inventory.slot(0, inventory::java::OFFHAND_MENU_SLOT),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Complete,
    Release,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effects {
    pub can_sprint: bool,
    pub speed_multiplier: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Active {
    pub ticks: u32,
    pub end: End,
    pub effects: Effects,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResultKind {
    Pass,
    Fail,
    /// Request is valid but this specialized native local result is not predicted.
    Unpredicted,
    Consume(Option<Active>),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub send: bool,
    pub result: ResultKind,
}
#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    pub food: Option<i32>,
    pub invulnerable: bool,
    pub creative: bool,
    pub projectile: bool,
}
pub(super) fn component<'a>(stack: &'a Stack, key: &str) -> Option<&'a Value> {
    if stack.removed.contains(key) {
        None
    } else {
        stack
            .added
            .get(key)
            .or_else(|| defaults::component(&stack.item, key))
    }
}
fn boolean(value: &Value, key: &str, default: bool) -> Result<bool, &'static str> {
    value.get(key).map_or(Ok(default), |v| {
        v.as_bool().ok_or("Invalid native use boolean")
    })
}
pub(super) fn effects(stack: &Stack) -> Result<Effects, &'static str> {
    let Some(value) = component(stack, "minecraft:use_effects") else {
        return Ok(Effects {
            can_sprint: false,
            speed_multiplier: 0.2,
        });
    };
    let multiplier = value
        .get("speed_multiplier")
        .map_or(Ok(0.2), |v| v.as_f64().ok_or("Invalid native use speed"))?
        as f32;
    if !multiplier.is_finite() || !(0. ..=1.).contains(&multiplier) {
        return Err("Invalid native use speed");
    }
    Ok(Effects {
        can_sprint: boolean(value, "can_sprint", false)?,
        speed_multiplier: multiplier,
    })
}
/// Native Consumable.consumeTicks uses f32 multiplication followed by Java f2i.
fn consume_ticks(value: &Value) -> Result<u32, &'static str> {
    let seconds = value
        .get("seconds")
        .or_else(|| value.get("consume_seconds"))
        .map_or(Ok(1.6), |v| {
            v.as_f64().ok_or("Invalid native consume duration")
        })? as f32;
    if !seconds.is_finite() || seconds < 0. {
        return Err("Invalid native consume duration");
    }
    Ok((seconds * 20.) as i32 as u32)
}

/// Refreshes only current use properties, without re-running Item.use admission or
/// resetting the already running remaining-tick countdown (LivingEntity.updatingUsingItem).
pub(super) fn current_duration(stack: &Stack) -> Result<u32, &'static str> {
    let rules =
        assets::registry::java::use_rules::builtin().map_err(|_| "Invalid native use rules")?;
    let owner = &rules
        .item(&stack.item)
        .ok_or("Unknown native item-use identity")?
        .air_owner;
    if owner == "Item" {
        if let Some(value) = component(stack, "minecraft:consumable") {
            return consume_ticks(value);
        }
        return Ok(
            if component(stack, "minecraft:blocks_attacks").is_some()
                || component(stack, "minecraft:kinetic_weapon").is_some()
            {
                72000
            } else {
                0
            },
        );
    }
    if owner == "BowItem" {
        let id = java_protocol::target::ITEM_TYPES
            .iter()
            .position(|name| *name == stack.item)
            .ok_or("Unknown native item-use defaults")?;
        return Ok(java_protocol::target::ITEM_USE_DEFAULTS[id].duration);
    }
    Err("Native specialized active item-use semantics are not implemented")
}

fn cooldown(
    stack: &Stack,
    defaults: Defaults<'_>,
    cooldowns: &Cooldowns,
) -> Result<bool, &'static str> {
    let key = "minecraft:use_cooldown";
    let group = if let Some(value) = stack.added.get(key) {
        let object = value
            .as_object()
            .ok_or("Invalid native item cooldown component")?;
        match object.get("group").or_else(|| object.get("cooldown_group")) {
            Some(Value::String(group)) => group.as_str(),
            None | Some(Value::Null) => stack.item.as_str(),
            _ => return Err("Invalid native item cooldown group"),
        }
    } else if stack.removed.contains(key) {
        stack.item.as_str()
    } else {
        defaults.group
    };
    Ok(cooldowns.active(group))
}
/// Uses exact base Item.use precedence. Specialized native classes remain explicit;
/// getUseDuration alone cannot establish their ammo/charge/start behavior.
pub fn prepare(
    held: Option<&Slot>,
    native: Option<Defaults<'_>>,
    cooldowns: &Cooldowns,
    c: Conditions,
) -> Result<Plan, &'static str> {
    let slot = held.ok_or("Native held item is not known")?;
    let Some(stack) = slot else {
        return Ok(Plan {
            send: false,
            result: ResultKind::Pass,
        });
    };
    let native = native.ok_or("Unknown native item-use defaults")?;
    if cooldown(stack, native, cooldowns)? {
        // In26.3 the prediction closure still creates UseItem on cooldown,
        // returning local PASS. The four-tick click cadence remains caller-owned.
        return Ok(Plan {
            send: true,
            result: ResultKind::Pass,
        });
    }
    let rules =
        assets::registry::java::use_rules::builtin().map_err(|_| "Invalid native use rules")?;
    let owner = &rules
        .item(&stack.item)
        .ok_or("Unknown native item-use identity")?
        .air_owner;
    if owner == "Item" {
        if let Some(value) = component(stack, "minecraft:consumable") {
            if let Some(food) = component(stack, "minecraft:food") {
                let always = boolean(food, "can_always_eat", false)?;
                if !c.invulnerable && !always && c.food.ok_or("Native hunger is not known")? >= 20 {
                    return Ok(Plan {
                        send: true,
                        result: ResultKind::Fail,
                    });
                }
            }
            let ticks = consume_ticks(value)?;
            return Ok(Plan {
                send: true,
                result: ResultKind::Consume(if ticks == 0 {
                    None
                } else {
                    Some(Active {
                        ticks,
                        end: End::Complete,
                        effects: effects(stack)?,
                    })
                }),
            });
        }
        if let Some(value) = component(stack, "minecraft:equippable")
            && boolean(value, "swappable", true)?
        {
            return Ok(Plan {
                send: true,
                result: ResultKind::Consume(None),
            });
        }
        if component(stack, "minecraft:blocks_attacks").is_some() {
            return Ok(Plan {
                send: true,
                result: ResultKind::Consume(Some(Active {
                    ticks: 72000,
                    end: End::Complete,
                    effects: effects(stack)?,
                })),
            });
        }
        if component(stack, "minecraft:kinetic_weapon").is_some() {
            return Err("Native kinetic-weapon use is not implemented");
        }
        return Ok(Plan {
            send: true,
            result: ResultKind::Pass,
        });
    }
    if owner == "BowItem" {
        if !c.creative && !c.projectile {
            return Ok(Plan {
                send: true,
                result: ResultKind::Fail,
            });
        }
        return Ok(Plan {
            send: true,
            result: ResultKind::Consume(Some(Active {
                ticks: native.duration,
                end: End::Release,
                effects: effects(stack)?,
            })),
        });
    }
    if native.duration != 0 {
        return Err("Native specialized active item-use semantics are not implemented");
    }
    // Existing native instant-use path: the server alone decides results. Its PASS/
    // consume result cannot be inferred merely from a zero getUseDuration value.
    Ok(Plan {
        send: true,
        result: ResultKind::Unpredicted,
    })
}

/// Base-dispatch fixture entry; production supplies target facts through block::at.
#[cfg(test)]
pub fn block_passes(block: &str, held: &Slot, secondary: bool) -> Result<bool, &'static str> {
    super::block::passes(block, held, secondary, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn stack(item: &str) -> Slot {
        Some(Stack {
            item: item.into(),
            count: 2,
            added: Default::default(),
            removed: Default::default(),
        })
    }
    fn conditions() -> Conditions {
        Conditions {
            food: Some(19),
            invulnerable: false,
            creative: false,
            projectile: true,
        }
    }
    fn native(duration: u32) -> Option<Defaults<'static>> {
        Some(Defaults {
            duration,
            group: "fixture:cooldown",
        })
    }
    #[test]
    fn consume_and_shield_plans_follow_components_without_consuming_stacks() {
        let mut held = stack("minecraft:apple");
        let before = held.clone();
        let c = Cooldowns::default();
        let p = prepare(Some(&held), native(32), &c, conditions()).unwrap();
        assert_eq!(
            p,
            Plan {
                send: true,
                result: ResultKind::Consume(Some(Active {
                    ticks: 32,
                    end: End::Complete,
                    effects: Effects {
                        can_sprint: false,
                        speed_multiplier: 0.2
                    }
                }))
            }
        );
        assert_eq!(held, before);
        assert_eq!(
            prepare(
                Some(&held),
                native(32),
                &c,
                Conditions {
                    food: Some(20),
                    ..conditions()
                }
            )
            .unwrap()
            .result,
            ResultKind::Fail
        );
        assert!(
            prepare(
                Some(&held),
                native(32),
                &c,
                Conditions {
                    food: Some(20),
                    ..conditions()
                }
            )
            .unwrap()
            .send,
            "26.3 sends a request even when the native local result is FAIL"
        );
        held.as_mut()
            .unwrap()
            .removed
            .insert("minecraft:consumable".into());
        assert_eq!(
            prepare(Some(&held), native(32), &c, conditions())
                .unwrap()
                .result,
            ResultKind::Pass
        );
        let held = stack("minecraft:shield");
        assert!(matches!(
            prepare(Some(&held), native(72000), &c, conditions())
                .unwrap()
                .result,
            ResultKind::Consume(Some(Active {
                ticks: 72000,
                end: End::Complete,
                ..
            }))
        ));
    }
    #[test]
    fn authoritative_duration_effect_and_cooldown_patches_override_defaults() {
        let mut held = stack("minecraft:potion");
        let stack = held.as_mut().unwrap();
        stack
            .added
            .insert("minecraft:consumable".into(), json!({"seconds":0.75}));
        stack.added.insert(
            "minecraft:use_effects".into(),
            json!({"can_sprint":true,"speed_multiplier":0.5}),
        );
        let mut c = Cooldowns::default();
        assert_eq!(
            prepare(
                Some(&held),
                native(32),
                &c,
                Conditions {
                    food: Some(20),
                    ..conditions()
                }
            )
            .unwrap()
            .result,
            ResultKind::Consume(Some(Active {
                ticks: 15,
                end: End::Complete,
                effects: Effects {
                    can_sprint: true,
                    speed_multiplier: 0.5
                }
            }))
        );
        c.set("fixture:cooldown".into(), 20).unwrap();
        assert_eq!(
            prepare(Some(&held), native(32), &c, conditions()).unwrap(),
            Plan {
                send: true,
                result: ResultKind::Pass
            }
        );
        held.as_mut()
            .unwrap()
            .removed
            .insert("minecraft:use_cooldown".into());
        assert!(matches!(
            prepare(Some(&held), native(32), &c, conditions())
                .unwrap()
                .result,
            ResultKind::Consume(Some(_))
        ));
        held.as_mut()
            .unwrap()
            .added
            .insert("minecraft:consumable".into(), json!({"seconds":-1}));
        assert!(prepare(Some(&held), native(32), &c, conditions()).is_err());
    }
    #[test]
    fn bow_ammo_fail_and_explicit_specialized_gaps_are_not_instant_successes() {
        let c = Cooldowns::default();
        let bow = stack("minecraft:bow");
        assert_eq!(
            prepare(
                Some(&bow),
                native(72000),
                &c,
                Conditions {
                    projectile: false,
                    ..conditions()
                }
            )
            .unwrap(),
            Plan {
                send: true,
                result: ResultKind::Fail
            }
        );
        assert!(matches!(
            prepare(Some(&bow), native(72000), &c, conditions())
                .unwrap()
                .result,
            ResultKind::Consume(Some(Active {
                end: End::Release,
                ..
            }))
        ));
        assert!(matches!(
            prepare(
                Some(&bow),
                native(72000),
                &c,
                Conditions {
                    projectile: false,
                    creative: true,
                    ..conditions()
                }
            )
            .unwrap()
            .result,
            ResultKind::Consume(Some(_))
        ));
        assert!(
            prepare(
                Some(&stack("minecraft:crossbow")),
                native(72000),
                &c,
                conditions()
            )
            .is_err()
        );
        assert!(prepare(None, None, &c, conditions()).is_err());
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    #[test]
    fn block_overrides_and_transformers_do_not_get_an_invented_air_pass() {
        let mut held = Some(Stack {
            item: "minecraft:apple".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        });
        assert_eq!(block_passes("minecraft:stone", &held, false), Ok(true));
        assert_eq!(block_passes("minecraft:chest", &held, false), Ok(false));
        assert_eq!(block_passes("minecraft:chest", &held, true), Ok(true));
        held.as_mut()
            .unwrap()
            .added
            .insert("minecraft:block_transformer".into(), serde_json::json!({}));
        assert_eq!(block_passes("minecraft:stone", &held, false), Ok(false));
        held.as_mut()
            .unwrap()
            .removed
            .insert("minecraft:block_transformer".into());
        assert_eq!(block_passes("minecraft:stone", &held, false), Ok(true));
    }
    #[test]
    fn removed_use_effects_still_use_the_native_fallback() {
        let mut held = Stack {
            item: "minecraft:apple".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        };
        held.removed.insert("minecraft:use_effects".into());
        assert_eq!(
            effects(&held).unwrap(),
            Effects {
                can_sprint: false,
                speed_multiplier: 0.2
            }
        );
    }
}
