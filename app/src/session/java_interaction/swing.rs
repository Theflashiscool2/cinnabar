//! Local gesture prediction after ordered admission; never predicts hits or inventory.
use world::java::entities::animation::swing::{Kind, modified_duration};

fn description(held: Option<&inventory::java::Slot>) -> Option<(Kind, i32)> {
    let held = held?;
    let value = held.as_ref().and_then(|stack| {
        let name = "minecraft:attack_animation";
        if stack.removed.contains(name) {
            None
        } else {
            stack
                .added
                .get(name)
                .or_else(|| assets::registry::java::items::defaults::component(&stack.item, name))
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
    let Some((kind, duration)) = description(state.inventory.held()) else {
        return;
    };
    let modified = modified_duration(&state.player_dynamics.effects, duration);
    state
        .player_visual
        .swing
        .start(false, kind, duration, modified);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
