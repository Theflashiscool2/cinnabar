//! Native click prediction metadata. Only the server's ensuing packets change inventory authority.
use crate::{ProtocolError as Error, codec::write_varint, target};
use std::collections::BTreeMap;

/// A nonempty predicted stack; hashes are target HashOps/codec hashes, not network-byte checksums.
#[derive(Clone, Debug, PartialEq)]
pub struct HashedStack {
    pub item: u32,
    pub count: u32,
    pub added: BTreeMap<u32, i32>,
    pub removed: Vec<u32>,
}

impl HashedStack {
    /// Builds the hashes we can prove. Missing codec hashes force authoritative server resync;
    /// they never fabricate a matching hash or change local inventory authority.
    pub fn predicted(
        item: u32,
        count: u32,
        added: &BTreeMap<String, serde_json::Value>,
        removed: &std::collections::BTreeSet<String>,
    ) -> Result<(Self, usize), Error> {
        let component_id = |name: &str| {
            target::ITEM_COMPONENTS
                .iter()
                .position(|c| c.name == name)
                .map(|id| id as u32)
                .ok_or(Error::Invalid("unknown native predicted component"))
        };
        let mut hashes = BTreeMap::new();
        let mut missing = 0;
        for (name, value) in added {
            let id = component_id(name)?;
            match super::prediction::component(name, value) {
                Some(hash) => {
                    hashes.insert(id, hash);
                }
                None => missing += 1,
            }
        }
        let result = Self {
            item,
            count,
            added: hashes,
            removed: removed
                .iter()
                .map(|name| component_id(name))
                .collect::<Result<_, _>>()?,
        };
        result.validate()?;
        Ok((result, missing))
    }

    /// Unpatched stacks need no component hash. Refuse patches rather than inventing predictions.
    pub fn unpatched(stack: &super::Stack) -> Result<Self, Error> {
        if !stack.added.is_empty() || !stack.removed.is_empty() {
            return Err(Error::Invalid(
                "native patched-item prediction hashing not implemented",
            ));
        }
        let result = Self {
            item: stack.item,
            count: stack.count,
            added: BTreeMap::new(),
            removed: Vec::new(),
        };
        result.validate()?;
        Ok(result)
    }
    fn validate(&self) -> Result<(), Error> {
        if self.item == target::ITEM_EMPTY_ID
            || target::ITEM_TYPES.get(self.item as usize).is_none()
            || self.count == 0
            || self.count > i32::MAX as u32
            || self.added.len() > target::ITEM_COMPONENTS.len()
            || self.removed.len() > target::ITEM_COMPONENTS.len()
        {
            return Err(Error::Invalid("native hashed stack identity/count/patch"));
        }
        let mut removed = std::collections::BTreeSet::new();
        for id in self.added.keys().chain(self.removed.iter()) {
            if *id as usize >= target::ITEM_COMPONENTS.len() {
                return Err(Error::Invalid("native hashed component identity"));
            }
        }
        for id in &self.removed {
            if self.added.contains_key(id) || !removed.insert(*id) {
                return Err(Error::Invalid("duplicate native hashed component"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Pickup {
    pub container: i32,
    pub state: i32,
    pub slot: i16,
    /// Native ordinary pickup: zero is left, one is right. Other actions use distinct codecs.
    pub button: u8,
    pub changed: BTreeMap<i16, Option<HashedStack>>,
    pub carried: Option<HashedStack>,
}

/// Fully validated bytes, prepared before entering the connection's bounded command queue.
#[derive(Clone, Debug)]
pub struct PreparedClick {
    container: i32,
    bytes: Vec<u8>,
    continuation: Vec<Vec<u8>>,
}
impl PreparedClick {
    pub fn container(&self) -> i32 {
        self.container
    }
    pub fn payload(&self) -> &[u8] {
        &self.bytes
    }
    /// One admitted queue entry can contain an ordered quick-craft sequence.
    pub fn payloads(&self) -> impl Iterator<Item = &[u8]> {
        std::iter::once(self.bytes.as_slice()).chain(self.continuation.iter().map(Vec::as_slice))
    }
}

pub fn pickup(click: &Pickup) -> Result<PreparedClick, Error> {
    if click.button > 1 {
        return Err(Error::Invalid("native pickup button"));
    }
    encode(click, target::CONTAINER_INPUT_PICKUP)
}

fn encode(click: &Pickup, action: u8) -> Result<PreparedClick, Error> {
    if click.container < 0
        || click.state < 0
        || (click.slot < 0
            && !(action == target::CONTAINER_INPUT_QUICK_CRAFT
                && click.slot == -999
                && matches!(click.button, 0 | 2 | 4 | 6)))
        || click.changed.len() > target::CONTAINER_MAX_CHANGED_SLOTS
        || click.changed.keys().any(|slot| *slot < 0)
    {
        return Err(Error::Invalid("native pickup fields"));
    }
    for stack in click
        .changed
        .values()
        .chain(std::iter::once(&click.carried))
        .flatten()
    {
        stack.validate()?;
    }
    let mut bytes = Vec::new();
    write_varint(click.container, &mut bytes);
    write_varint(click.state, &mut bytes);
    bytes.extend(click.slot.to_be_bytes());
    bytes.push(click.button);
    write_varint(i32::from(action), &mut bytes);
    write_varint(click.changed.len() as i32, &mut bytes);
    for (slot, stack) in &click.changed {
        bytes.extend(slot.to_be_bytes());
        hashed(stack.as_ref(), &mut bytes);
    }
    hashed(click.carried.as_ref(), &mut bytes);
    Ok(PreparedClick {
        container: click.container,
        bytes,
        continuation: Vec::new(),
    })
}

pub fn close(container: i32) -> Result<Vec<u8>, Error> {
    if container < 0 {
        return Err(Error::Invalid("native close container identity"));
    }
    let mut bytes = Vec::new();
    write_varint(container, &mut bytes);
    Ok(bytes)
}

fn hashed(stack: Option<&HashedStack>, bytes: &mut Vec<u8>) {
    let Some(stack) = stack else {
        bytes.push(0);
        return;
    };
    bytes.push(1);
    // ByteBufCodecs.holderRegistry uses the registry ID directly (unlike inline-holder codecs).
    write_varint(stack.item as i32, bytes);
    write_varint(stack.count as i32, bytes);
    write_varint(stack.added.len() as i32, bytes);
    for (id, hash) in &stack.added {
        write_varint(*id as i32, bytes);
        bytes.extend(hash.to_be_bytes());
    }
    write_varint(stack.removed.len() as i32, bytes);
    for id in &stack.removed {
        write_varint(*id as i32, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stone(count: u32) -> HashedStack {
        HashedStack {
            item: target::ITEM_TYPES
                .iter()
                .position(|n| *n == "minecraft:stone")
                .unwrap() as u32,
            count,
            added: BTreeMap::new(),
            removed: Vec::new(),
        }
    }
    #[test]
    fn ordinary_pickup_and_place_match_independent_native_packet_vectors() {
        let first = Pickup {
            container: 7,
            state: 129,
            slot: 0,
            button: 0,
            changed: BTreeMap::from([(0, None)]),
            carried: Some(stone(8)),
        };
        assert_eq!(
            pickup(&first).unwrap().payload(),
            [7, 129, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 8, 0, 0]
        );
        let next = Pickup {
            slot: 1,
            button: 1,
            changed: BTreeMap::from([(1, Some(stone(1)))]),
            carried: Some(stone(7)),
            ..first
        };
        assert_eq!(
            pickup(&next).unwrap().payload(),
            [7, 129, 1, 0, 1, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 7, 0, 0]
        );
        assert_eq!(close(129).unwrap(), [129, 1]);
    }
    #[test]
    fn malformed_predictions_are_rejected_before_the_connection_queue() {
        let mut click = Pickup {
            container: 3,
            state: 0,
            slot: 0,
            button: 0,
            changed: BTreeMap::new(),
            carried: Some(stone(1)),
        };
        click.carried.as_mut().unwrap().count = 0;
        assert!(pickup(&click).is_err());
        click.carried = None;
        click.button = 2;
        assert!(pickup(&click).is_err());
        assert!(close(-1).is_err());
    }
}

#[cfg(test)]
mod prediction_tests {
    use super::*;
    #[test]
    fn patched_predictions_retain_known_hashes_and_require_resync_for_unknown_forms() {
        let item = target::ITEM_TYPES
            .iter()
            .position(|n| *n == "minecraft:stone")
            .unwrap() as u32;
        let added = BTreeMap::from([
            (
                "minecraft:custom_name".into(),
                serde_json::json!({"String":"Hi"}),
            ),
            (
                "minecraft:attribute_modifiers".into(),
                serde_json::json!([]),
            ),
        ]);
        let (stack, missing) =
            HashedStack::predicted(item, 4, &added, &Default::default()).unwrap();
        let name = target::ITEM_COMPONENTS
            .iter()
            .position(|s| s.name == "minecraft:custom_name")
            .unwrap() as u32;
        assert_eq!(stack.added, BTreeMap::from([(name, 485089409)]));
        assert_eq!(missing, 1, "missing codec hashes are never fabricated");
        let click = pickup(&Pickup {
            container: 1,
            state: 7,
            slot: 3,
            button: 0,
            changed: BTreeMap::from([(3, None)]),
            carried: Some(stack),
        })
        .unwrap();
        assert_eq!(click.container(), 1);
        assert_eq!(
            &click.payload()[..6],
            &[1, 7, 0, 3, 0, target::CONTAINER_INPUT_PICKUP]
        );
        assert!(
            HashedStack::predicted(
                item,
                4,
                &BTreeMap::from([("test:unknown".into(), serde_json::Value::Null)]),
                &Default::default()
            )
            .is_err()
        );
    }
}

/// Serializes an action when the client has installed no local prediction. Empty changes and
/// the observed pre-action cursor describe its real view, so the server broadcasts the result.
pub fn observed_pickup(
    container: i32,
    state: i32,
    slot: i16,
    button: u8,
    carried: Option<HashedStack>,
) -> Result<PreparedClick, Error> {
    pickup(&Pickup {
        container,
        state,
        slot,
        button,
        changed: BTreeMap::new(),
        carried,
    })
}
#[cfg(test)]
mod observed_tests {
    use super::*;
    #[test]
    fn unpredicted_action_never_claims_the_post_click_cursor_or_changed_slots() {
        // An initially empty cursor and unchanged local slots. The server must compute the
        // pickup and return its deltas, rather than being told we already display the result.
        let click = observed_pickup(1, 7, 3, 0, None).unwrap();
        assert_eq!(click.payload(), &[1, 7, 0, 3, 0, 0, 0, 0]);
        let stone = target::ITEM_TYPES
            .iter()
            .position(|v| *v == "minecraft:stone")
            .unwrap() as u32;
        let carried = HashedStack {
            item: stone,
            count: 4,
            added: Default::default(),
            removed: Vec::new(),
        };
        let click = observed_pickup(1, 8, 4, 1, Some(carried)).unwrap();
        assert_eq!(click.payload(), &[1, 8, 0, 4, 1, 0, 0, 1, 1, 4, 0, 0]);
    }
}

/// Server-executed storage gestures. No slot or cursor prediction is installed locally.
#[derive(Clone, Copy, Debug)]
pub enum ObservedAction {
    QuickMove,
    Collect,
    HotbarSwap(u8),
    Throw { whole_stack: bool },
}

pub fn observed_action(
    container: i32,
    state: i32,
    slot: i16,
    action: ObservedAction,
    carried: Option<HashedStack>,
) -> Result<PreparedClick, Error> {
    let (mode, button) = match action {
        ObservedAction::QuickMove => (target::CONTAINER_INPUT_QUICK_MOVE, 0),
        ObservedAction::Collect if carried.is_some() => (target::CONTAINER_INPUT_PICKUP_ALL, 0),
        ObservedAction::Collect => {
            return Err(Error::Invalid("native collect requires a carried stack"));
        }
        ObservedAction::HotbarSwap(button) if button < 9 => (target::CONTAINER_INPUT_SWAP, button),
        ObservedAction::Throw { whole_stack } => {
            (target::CONTAINER_INPUT_THROW, u8::from(whole_stack))
        }
        ObservedAction::HotbarSwap(_) => return Err(Error::Invalid("native hotbar swap button")),
    };
    encode(
        &Pickup {
            container,
            state,
            slot,
            button,
            changed: BTreeMap::new(),
            carried,
        },
        mode,
    )
}

#[cfg(test)]
mod storage_action_tests {
    use super::*;
    #[test]
    fn collect_matching_uses_native_mode_and_the_actual_pre_action_carried_stack() {
        let carried = HashedStack {
            item: 1,
            count: 2,
            added: BTreeMap::new(),
            removed: Vec::new(),
        };
        assert_eq!(
            observed_action(3, 129, 53, ObservedAction::Collect, Some(carried))
                .unwrap()
                .payload(),
            &[3, 129, 1, 0, 53, 0, 6, 0, 1, 1, 2, 0, 0]
        );
        assert!(observed_action(3, 129, 53, ObservedAction::Collect, None).is_err());
    }
    #[test]
    fn server_storage_actions_have_distinct_wire_modes_and_no_claimed_predictions() {
        for (action, button, mode) in [
            (ObservedAction::QuickMove, 0, 1),
            (ObservedAction::HotbarSwap(8), 8, 2),
            (ObservedAction::Throw { whole_stack: false }, 0, 4),
            (ObservedAction::Throw { whole_stack: true }, 1, 4),
        ] {
            assert_eq!(
                observed_action(3, 129, 53, action, None).unwrap().payload(),
                &[3, 129, 1, 0, 53, button, mode, 0, 0]
            );
        }
        assert!(observed_action(1, 0, 0, ObservedAction::HotbarSwap(9), None).is_err());
        assert!(observed_action(1, 0, -1, ObservedAction::QuickMove, None).is_err());
        assert!(observed_action(1, -1, 0, ObservedAction::QuickMove, None).is_err());
    }
}

/// Prepares every quick-craft phase before queue admission; no local item changes are claimed.
pub fn quick_craft(
    container: i32,
    state: i32,
    slots: &[i16],
    one_each: bool,
    carried: HashedStack,
) -> Result<PreparedClick, Error> {
    if slots.len() < 2
        || slots.len() > target::CONTAINER_MAX_CHANGED_SLOTS
        || slots.iter().any(|s| *s < 0)
        || slots
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != slots.len()
    {
        return Err(Error::Invalid("native quick-craft slots"));
    }
    let kind: u8 = if one_each { 4 } else { 0 };
    let phase = |slot: i16, header: u8| {
        encode(
            &Pickup {
                container,
                state,
                slot,
                button: kind | header,
                changed: BTreeMap::new(),
                carried: Some(carried.clone()),
            },
            target::CONTAINER_INPUT_QUICK_CRAFT,
        )
    };
    let mut first = phase(-999, 0)?;
    for slot in slots {
        first.continuation.push(phase(*slot, 1)?.bytes);
    }
    first.continuation.push(phase(-999, 2)?.bytes);
    Ok(first)
}

#[cfg(test)]
mod quick_craft_tests {
    use super::*;
    #[test]
    fn drag_encodes_exact_phase_order_buttons_and_observed_cursor() {
        for (one_each, buttons) in [(false, [0, 1, 1, 2]), (true, [4, 5, 5, 6])] {
            let carried = HashedStack {
                item: 1,
                count: 11,
                added: BTreeMap::new(),
                removed: Vec::new(),
            };
            let click = quick_craft(3, 7, &[2, 4], one_each, carried).unwrap();
            let payloads: Vec<_> = click.payloads().collect();
            assert_eq!(payloads.len(), 4);
            for (i, prefix) in [[0xfc, 0x19], [0, 2], [0, 4], [0xfc, 0x19]]
                .into_iter()
                .enumerate()
            {
                assert_eq!(
                    payloads[i],
                    &[3, 7, prefix[0], prefix[1], buttons[i], 5, 0, 1, 1, 11, 0, 0]
                );
            }
        }
    }
    #[test]
    fn malformed_drag_never_yields_a_partial_start() {
        let carried = || HashedStack {
            item: 1,
            count: 3,
            added: BTreeMap::new(),
            removed: Vec::new(),
        };
        for slots in [&[][..], &[1][..], &[1, 1][..], &[-999, 2][..]] {
            assert!(quick_craft(1, 0, slots, false, carried()).is_err());
        }
        assert!(
            quick_craft(
                1,
                0,
                &vec![0; target::CONTAINER_MAX_CHANGED_SLOTS + 1],
                false,
                carried()
            )
            .is_err()
        );
        assert!(observed_pickup(1, 0, -999, 0, None).is_err());
    }
}
