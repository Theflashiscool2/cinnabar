//! Local gesture prediction after ordered admission; never predicts hits or inventory.
use gameplay::java::item_use::plan::Hand;
use world::java::entities::animation::swing::{Kind, modified_duration};

const ATTACK: &str = "minecraft:attack_animation";
const INTERACT: &str = "minecraft:interact_animation";

fn description(held: Option<&inventory::java::Slot>) -> Option<(Kind, i32)> {
    describe(held, ATTACK)
}

fn describe(held: Option<&inventory::java::Slot>, component: &str) -> Option<(Kind, i32)> {
    let held = held?;
    let value = held.as_ref().and_then(|stack| {
        if stack.removed.contains(component) {
            None
        } else {
            stack.added.get(component).or_else(|| {
                assets::registry::java::items::defaults::component(&stack.item, component)
            })
        }
    });
    // ItemStack.getAttackAnimation falls back to SwingAnimation.DEFAULT, including removals.
    let Some(value) = value else {
        return Some((Kind::Whack, 6));
    };
    let value = value.as_object()?;
    let kind = match value.get("type") {
        None => Kind::Whack,
        Some(serde_json::Value::String(name)) => match name.as_str() {
            "none" => Kind::None,
            "whack" => Kind::Whack,
            "stab" => Kind::Stab,
            _ => return None,
        },
        Some(id) => Kind::from_id(i32::try_from(id.as_i64()?).ok()?),
    };
    let duration = value
        .get("duration")
        .map_or(Some(6), |n| i32::try_from(n.as_i64()?).ok())?;
    Some((kind, duration))
}
pub(super) fn predict(state: &mut client_world::java::WorldState) {
    let animation = description(state.inventory.held());
    start(state, false, animation);
}

/// 26.3 startUseItem uses getInteractAnimation, independently of attack gestures.
/// Ordinary placement feedback follows ordered admission; unresolved entity collision
/// is cosmetic attempt feedback, never a successful placement prediction.
pub(super) fn predict_interact(state: &mut client_world::java::WorldState, hand: Hand) {
    let animation = describe(hand.held(&state.inventory), INTERACT);
    start(state, hand == Hand::Off, animation);
}

fn start(
    state: &mut client_world::java::WorldState,
    offhand: bool,
    animation: Option<(Kind, i32)>,
) {
    let Some((kind, duration)) = animation else {
        return;
    };
    let modified = modified_duration(&state.player_dynamics.effects, duration);
    state
        .player_visual
        .swing
        .start(offhand, kind, duration, modified);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn interaction_animation_is_independent_of_attack_and_respects_removal() {
        let mut stack = inventory::java::Stack {
            item: "fixture:item".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        };
        stack
            .added
            .insert(ATTACK.into(), json!({"type":"stab","duration":12}));
        stack
            .added
            .insert(INTERACT.into(), json!({"type":"whack","duration":4}));
        assert_eq!(
            description(Some(&Some(stack.clone()))),
            Some((Kind::Stab, 12))
        );
        assert_eq!(
            describe(Some(&Some(stack.clone())), INTERACT),
            Some((Kind::Whack, 4))
        );
        stack.removed.insert(INTERACT.into());
        assert_eq!(
            describe(Some(&Some(stack.clone())), INTERACT),
            Some((Kind::Whack, 6))
        );
        assert_eq!(description(Some(&Some(stack))), Some((Kind::Stab, 12)));
        assert_eq!(describe(None, INTERACT), None);
    }
    #[test]
    fn native_default_patch_removal_and_malformed_animation() {
        assert_eq!(description(None), None);
        assert_eq!(description(Some(&None)), Some((Kind::Whack, 6)));
        let mut stack = inventory::java::Stack {
            item: "minecraft:diamond_sword".into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        };
        assert_eq!(
            description(Some(&Some(stack.clone()))),
            Some((Kind::Whack, 6))
        );
        for (value, expected) in [
            (json!({}), Some((Kind::Whack, 6))),
            (json!("untrusted"), None),
            (json!([]), None),
            (json!({"duration": 120}), Some((Kind::Whack, 120))),
            (json!({"type": 1,"duration": 120}), Some((Kind::Whack, 120))),
            (json!({"type": "stab","duration": 9}), Some((Kind::Stab, 9))),
            (json!({"type": 0,"duration": -2}), Some((Kind::None, -2))),
            (json!({"type": "whack"}), Some((Kind::Whack, 6))),
            (json!({"type": "invalid"}), None),
            (json!({"type": 1,"duration": "untrusted"}), None),
        ] {
            stack
                .added
                .insert("minecraft:attack_animation".into(), value);
            assert_eq!(description(Some(&Some(stack.clone()))), expected);
        }
        stack.removed.insert("minecraft:attack_animation".into());
        assert_eq!(description(Some(&Some(stack))), Some((Kind::Whack, 6)));
    }
}
