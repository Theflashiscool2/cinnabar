//! Native client-side entity fallthrough. The application resolves registry/tag
//! identities; this policy selects follow-up requests without mutating server state.
use super::plan::component;
use inventory::java::Slot;

#[derive(Clone, Copy)]
pub enum Target {
    Player,
    Pig {
        food: bool,
        saddled: bool,
        occupied: bool,
        secondary: bool,
    },
    Unsupported,
}
/// A false result still dispatches entity interaction, but never guesses PASS
/// into air use or another hand when the local native result consumes or is unknown.
pub fn passes(held: &Slot, target: Target) -> Result<bool, &'static str> {
    match target {
        Target::Pig {
            food,
            saddled,
            occupied,
            secondary,
        } => {
            // Animal consumes food client-side even if the server refuses breeding.
            // Pig prioritizes food over an unoccupied saddled mount.
            if food || (saddled && !occupied && !secondary) {
                return Ok(false);
            }
        }
        Target::Player => {}
        Target::Unsupported => return Ok(false),
    }
    let Some(stack) = held else { return Ok(true) };
    if let Some(equip) = component(stack, "minecraft:equippable") {
        let enabled = equip.get("equip_on_interact").map_or(Ok(false), |v| {
            v.as_bool().ok_or("Invalid native equip-on-interact")
        })?;
        if enabled {
            // Full target-slot fallthrough needs allowed holders and native slot
            // compatibility; don't start using the other hand on a guessed PASS.
            return Ok(false);
        }
    }
    let rules =
        assets::registry::java::use_rules::builtin().map_err(|_| "Invalid native use rules")?;
    let item = rules
        .item(&stack.item)
        .ok_or("Unknown native entity-use item")?;
    Ok(match item.living_owner.as_str() {
        "Item" | "DyeItem" => true, // Neither supported target is a dyeable animal.
        "NameTagItem" => {
            matches!(target, Target::Player) || component(stack, "minecraft:custom_name").is_none()
        }
        _ => false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use inventory::java::Stack;
    use serde_json::json;
    fn stack(item: &str) -> Slot {
        Some(Stack {
            item: item.into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        })
    }
    fn pig() -> Target {
        Target::Pig {
            food: false,
            saddled: false,
            occupied: false,
            secondary: false,
        }
    }
    #[test]
    fn native_living_dispatch_preserves_player_pass_and_consuming_item_precedence() {
        let mut name = stack("minecraft:name_tag");
        assert!(passes(&name, pig()).unwrap());
        name.as_mut()
            .unwrap()
            .added
            .insert("minecraft:custom_name".into(), json!({"text":"Pet"}));
        assert!(!passes(&name, pig()).unwrap());
        assert!(passes(&name, Target::Player).unwrap());
        for name in [
            "minecraft:iron_sword",
            "minecraft:shield",
            "minecraft:red_dye",
        ] {
            assert!(passes(&stack(name), Target::Player).unwrap());
            assert!(passes(&stack(name), pig()).unwrap());
        }
        assert!(!passes(&stack("minecraft:saddle"), pig()).unwrap());
        assert!(passes(&None, Target::Player).unwrap());
        assert!(!passes(&None, Target::Unsupported).unwrap());
    }
    #[test]
    fn food_and_riding_precede_item_air_use_even_with_secondary_or_occupied_targets() {
        let carrot = stack("minecraft:carrot");
        for secondary in [false, true] {
            for occupied in [false, true] {
                assert!(
                    !passes(
                        &carrot,
                        Target::Pig {
                            food: true,
                            saddled: true,
                            occupied,
                            secondary
                        }
                    )
                    .unwrap()
                );
            }
        }
        assert!(
            !passes(
                &None,
                Target::Pig {
                    food: false,
                    saddled: true,
                    occupied: false,
                    secondary: false
                }
            )
            .unwrap()
        );
        assert!(
            passes(
                &None,
                Target::Pig {
                    food: false,
                    saddled: true,
                    occupied: true,
                    secondary: false
                }
            )
            .unwrap()
        );
        assert!(
            passes(
                &None,
                Target::Pig {
                    food: false,
                    saddled: true,
                    occupied: false,
                    secondary: true
                }
            )
            .unwrap()
        );
    }
}
