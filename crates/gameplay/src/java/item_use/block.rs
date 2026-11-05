//! Target-native block/item precedence. Packets and authoritative stack/block changes
//! remain owned by the existing interaction/session path.
use super::plan::component;
use assets::registry::java::items::defaults;
use inventory::java::Slot;

pub fn at(
    state: &client_world::java::WorldState,
    hit: sim::java::selection::Hit,
    secondary: bool,
) -> Result<bool, &'static str> {
    let name = state
        .block_name(hit.state)
        .ok_or("Native use registry is not installed")?;
    let held = state
        .inventory
        .held()
        .ok_or("Native held item is not known")?;
    let target = assets::registry::java::mining::builtin()
        .ok()
        .and_then(|facts| facts.state(hit.state))
        .zip(
            state
                .bootstrap
                .as_ref()
                .and_then(|b| b.configuration.tags.get("minecraft:block")),
        )
        .map(|(facts, tags)| Target {
            down: hit.face == sim::java::selection::Face::Down,
            convertible_to_mud: tags
                .get("minecraft:convertible_to_mud")
                .is_some_and(|ids| ids.contains(&(facts.block as i32))),
        });
    passes(name, held, secondary, target)
}

/// Known target context for PotionItem's conditional useOn. None means that
/// required target/tag facts are unavailable, never permission to guess PASS.
#[derive(Clone, Copy)]
pub struct Target {
    pub down: bool,
    pub convertible_to_mud: bool,
}
pub fn passes(
    block: &str,
    held: &Slot,
    secondary: bool,
    potion_target: Option<Target>,
) -> Result<bool, &'static str> {
    let rules =
        assets::registry::java::use_rules::builtin().map_err(|_| "Invalid native use rules")?;
    let block = rules
        .block(block)
        .ok_or("Unknown native block-use identity")?;
    let Some(stack) = held else {
        return Ok(false);
    };
    let item = rules
        .item(&stack.item)
        .ok_or("Unknown native item-use identity")?;
    if !(secondary || (block.item_default && block.empty_default)) {
        return Ok(false);
    }
    if item.on_owner == "PotionItem" {
        let Some(target) = potion_target else {
            return Ok(false);
        };
        if target.down || !target.convertible_to_mud {
            return Ok(true);
        }
        let Some(contents) = component(stack, "minecraft:potion_contents") else {
            return Ok(true); // Native getOrDefault(EMPTY) cannot match WATER.
        };
        return defaults::is_plain_water(contents).map(|water| !water);
    }
    Ok(item.on_owner == "Item" && component(stack, "minecraft:block_transformer").is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inventory::java::Stack;
    use serde_json::json;
    fn potion(contents: serde_json::Value) -> Slot {
        Some(Stack {
            item: "minecraft:potion".into(),
            count: 1,
            added: [("minecraft:potion_contents".into(), contents)].into(),
            removed: Default::default(),
        })
    }
    #[test]
    fn potion_dispatch_preserves_mud_conversion_and_drinking_precedence() {
        // Native 26.3 registry WATER=0; PotionContents.is also requires no custom effects.
        let water = potion(json!({"potion":0,"effects":[]}));
        let mud = Some(Target {
            down: false,
            convertible_to_mud: true,
        });
        assert_eq!(passes("minecraft:dirt", &water, false, mud), Ok(false));
        assert_eq!(
            passes(
                "minecraft:dirt",
                &water,
                false,
                Some(Target {
                    down: true,
                    convertible_to_mud: true
                })
            ),
            Ok(true)
        );
        assert_eq!(
            passes(
                "minecraft:stone",
                &water,
                false,
                Some(Target {
                    down: false,
                    convertible_to_mud: false
                })
            ),
            Ok(true)
        );
        assert_eq!(
            passes("minecraft:dirt", &potion(json!({"potion":24})), false, mud),
            Ok(true)
        );
        assert_eq!(
            passes(
                "minecraft:dirt",
                &potion(json!({"potion":0,"effects":[{"id":0}]})),
                false,
                mud
            ),
            Ok(true)
        );
        assert_eq!(
            passes(
                "minecraft:dirt",
                &potion(json!({"potion":"minecraft:water"})),
                false,
                mud
            ),
            Ok(false)
        );
        assert_eq!(passes("minecraft:chest", &water, false, mud), Ok(false));
        assert_eq!(
            passes(
                "minecraft:chest",
                &water,
                true,
                Some(Target {
                    down: false,
                    convertible_to_mud: false
                })
            ),
            Ok(true)
        );
    }
    #[test]
    fn missing_unknown_and_removed_potion_facts_do_not_fabricate_conversion() {
        let mud = Some(Target {
            down: false,
            convertible_to_mud: true,
        });
        let mut water = potion(json!({"potion":0}));
        assert_eq!(passes("minecraft:dirt", &water, false, None), Ok(false));
        water
            .as_mut()
            .unwrap()
            .removed
            .insert("minecraft:potion_contents".into());
        assert_eq!(passes("minecraft:dirt", &water, false, mud), Ok(true));
        assert!(
            passes(
                "minecraft:dirt",
                &potion(json!({"potion":999999})),
                false,
                mud
            )
            .is_err()
        );
        assert!(
            passes(
                "minecraft:dirt",
                &potion(json!({"potion":0,"effects":"bad"})),
                false,
                mud
            )
            .is_err()
        );
    }
}
