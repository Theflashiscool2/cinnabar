//! Ordinary generic-container pickup plans; authority changes only through server updates.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MenuIdentity {
    pub session: u64,
    pub epoch: u64,
    pub container: i32,
    pub incarnation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub identity: MenuIdentity,
    pub state: i32,
    pub slot: usize,
    pub button: Button,
    pub changed: BTreeMap<usize, Slot>,
    pub carried: Slot,
}
/// A local socket write result. It never establishes server acceptance of item movement.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Receipt {
    pub identity: MenuIdentity,
    /// Session-scoped input ordering; unrelated to the server's wrapping state ID.
    pub sequence: u64,
    pub closed: bool,
    pub written: bool,
}
impl State {
    pub fn record_receipt(&mut self, receipt: Receipt) -> bool {
        if self.active_identity() != Some(receipt.identity)
            || self
                .input_receipt
                .is_some_and(|old| old.sequence >= receipt.sequence)
        {
            return false;
        }
        if self.pickup_writes.remove(&receipt.sequence) && !receipt.written {
            self.pickup_player_view.clear();
            self.clear_pickup_menu_view();
        }
        if receipt.written && receipt.closed {
            self.clear_pickup_menu_view();
            self.opened = None;
        }
        self.input_receipt = Some(receipt);
        self.revision = self.revision.saturating_add(1);
        true
    }
    pub fn opened_identity(&self) -> Option<MenuIdentity> {
        let menu = self.opened.as_ref()?;
        Some(MenuIdentity {
            session: self.session,
            epoch: self.epoch,
            container: menu.container,
            incarnation: menu.incarnation,
        })
    }
    /// The active server menu, or the personal menu when no foreign menu is open.
    pub fn active_identity(&self) -> Option<MenuIdentity> {
        self.opened_identity().or_else(|| {
            self.player.as_ref().map(|_| MenuIdentity {
                session: self.session,
                epoch: self.epoch,
                container: 0,
                incarnation: 0,
            })
        })
    }
    /// Current carried state for native outside-screen pickup. No local drop is installed.
    pub fn observe_carried(&self, identity: MenuIdentity) -> Result<(i32, &Slot), String> {
        if self.active_identity() != Some(identity) {
            return Err("Java container lifetime is no longer current".into());
        }
        if let Some(error) = &self.error {
            return Err(format!("Java inventory authority invalid: {error}"));
        }
        let menu = self
            .menu(identity.container)
            .ok_or("Java container is unknown")?;
        if menu.container != 0 && generic_storage_slots(&menu.kind).is_none() {
            return Err("Java specialized slot rules not implemented".into());
        }
        Ok((
            menu.state.ok_or("Java container contents are not ready")?,
            self.carried_view(identity.container)
                .ok_or("Java carried stack is unknown")?,
        ))
    }
    /// Native personal crafting/equipment clicks are executed by the server. Do not
    /// apply ordinary storage merge rules to output, recipe inputs or armor slots.
    pub fn observe_click_slot(
        &self,
        identity: MenuIdentity,
        slot: usize,
    ) -> Result<(i32, &Slot), String> {
        if identity.container != 0 || slot >= 9 {
            return self.observe_storage_slot(identity, slot);
        }
        if self.active_identity() != Some(identity) {
            return Err("Java container lifetime is no longer current".into());
        }
        self.slot(0, slot).ok_or("Java personal slot is unknown")?;
        self.observe_carried(identity)
    }
    /// Validates a current ordinary storage slot without predicting any item movement.
    /// Server quick-move/swap/throw rules remain authoritative; unknown/specialized slots refuse.
    pub fn observe_storage_slot(
        &self,
        identity: MenuIdentity,
        slot: usize,
    ) -> Result<(i32, &Slot), String> {
        if self.active_identity() != Some(identity) {
            return Err("Java container lifetime is no longer current".into());
        }
        if let Some(error) = &self.error {
            return Err(format!("Java inventory authority invalid: {error}"));
        }
        let menu = self.menu(identity.container).unwrap();
        if (menu.container == 0 && !(9..=OFFHAND_MENU_SLOT).contains(&slot))
            || (menu.container != 0 && !menu.kind.starts_with("GENERIC_9X"))
        {
            return Err("Java specialized slot rules not implemented".into());
        }
        self.slot(menu.container, slot)
            .ok_or("Java slot is unknown")?;
        self.observe_carried(identity)
    }
    /// Computes vanilla pickup metadata without mutating slots/cursor or declaring acceptance.
    /// Specialized slots (crafting, armor, outputs) require their own native placement rules.
    pub fn plan_pickup(
        &self,
        identity: MenuIdentity,
        slot: usize,
        button: Button,
        mut max_stack: impl FnMut(&Stack) -> Result<u32, String>,
    ) -> Result<Plan, String> {
        let (state, carried) = self.observe_storage_slot(identity, slot)?;
        let current = self.slot(identity.container, slot).unwrap();
        let mut next = current.clone();
        let mut cursor = carried.clone();
        match (current, carried) {
            (Some(item), None) => {
                let take = if button == Button::Right {
                    item.count / 2 + item.count % 2
                } else {
                    item.count
                };
                cursor = counted(item, take);
                next = counted(item, item.count - take);
            }
            (None, Some(item)) => {
                let limit = max_stack(item)?;
                if limit == 0 {
                    return Err("Java stack limit is unknown".into());
                }
                let count = if button == Button::Right {
                    1
                } else {
                    item.count.min(limit)
                };
                next = counted(item, count);
                cursor = counted(item, item.count - count);
            }
            (Some(item), Some(held)) => {
                let limit = max_stack(held)?;
                if limit == 0 {
                    return Err("Java stack limit is unknown".into());
                }
                if item.same_components(held) {
                    let available = limit.saturating_sub(item.count);
                    let take = available.min(if button == Button::Right {
                        1
                    } else {
                        held.count
                    });
                    next = counted(item, item.count + take);
                    cursor = counted(held, held.count - take);
                } else if held.count <= limit {
                    next = carried.clone();
                    cursor = current.clone();
                }
            }
            (None, None) => {}
        }
        let changed = if &next == current {
            BTreeMap::new()
        } else {
            BTreeMap::from([(slot, next)])
        };
        Ok(Plan {
            identity,
            state,
            slot,
            button,
            changed,
            carried: cursor,
        })
    }
}
fn counted(item: &Stack, count: u32) -> Slot {
    (count > 0).then(|| Stack {
        count,
        ..item.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed_cursor_is_current_known_and_does_not_drop_locally() {
        let mut s = menu(item(9), item(7));
        let id = s.active_identity().unwrap();
        let before = serde_json::to_value(&s).unwrap();
        assert_eq!(s.observe_carried(id).unwrap(), (19, &item(7)));
        assert_eq!(serde_json::to_value(&s).unwrap(), before);
        assert!(
            s.observe_carried(MenuIdentity {
                incarnation: id.incarnation + 1,
                ..id
            })
            .is_err()
        );
        s.opened.as_mut().unwrap().carried = None;
        assert!(s.observe_carried(id).is_err());
        s.begin(id.session, id.epoch + 1);
        assert!(s.observe_carried(id).is_err());
    }
    #[test]
    fn socket_receipts_never_change_authoritative_items_or_confirm_a_transfer() {
        let mut s = menu(item(9), None);
        let identity = s.opened_identity().unwrap();
        let authority = s.authority_revision;
        let receipt = Receipt {
            identity,
            sequence: 1,
            closed: false,
            written: true,
        };
        assert!(!s.record_receipt(Receipt {
            identity: MenuIdentity {
                incarnation: identity.incarnation + 1,
                ..identity
            },
            ..receipt
        }));
        assert!(s.record_receipt(receipt));
        assert_eq!(s.slot(identity.container, 0), Some(&item(9)));
        assert_eq!(s.opened.as_ref().unwrap().carried, Some(None));
        assert_eq!(s.authority_revision, authority);
        assert!(!s.record_receipt(receipt));
        assert!(s.record_receipt(Receipt {
            sequence: 2,
            closed: true,
            ..receipt
        }));
        assert!(s.opened.is_none());
        assert_eq!(s.authority_revision, authority);
    }
    #[test]
    fn observed_storage_gestures_require_known_current_contents_and_never_predict_transfers() {
        let mut s = menu(item(9), None);
        let id = s.active_identity().unwrap();
        let before = serde_json::to_value(&s).unwrap();
        assert_eq!(s.observe_storage_slot(id, 0).unwrap(), (19, &None));
        assert_eq!(s.observe_storage_slot(id, 62).unwrap(), (19, &None));
        assert!(s.observe_storage_slot(id, 63).is_err());
        assert!(
            s.observe_storage_slot(
                MenuIdentity {
                    incarnation: id.incarnation + 1,
                    ..id
                },
                0
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(&s).unwrap(), before);
        s.opened.as_mut().unwrap().carried = None;
        assert!(s.observe_storage_slot(id, 0).is_err());
        s.begin(7, 3);
        let id = s.active_identity().unwrap();
        assert!(s.observe_storage_slot(id, 36).is_err());
        assert!(s.observe_storage_slot(id, 5).is_err());
    }
    #[test]
    fn personal_offhand_pickup_uses_its_native_slot_and_preserves_authority() {
        let mut state = State::default();
        state.begin(7, 1);
        let mut slots = vec![None; PLAYER_MENU_SIZE];
        slots[OFFHAND_MENU_SLOT] = item(12);
        state
            .admit(
                7,
                1,
                Update::Content {
                    container: 0,
                    state: 4,
                    slots,
                    carried: None,
                },
            )
            .unwrap();
        let plan = state
            .plan_pickup(
                state.active_identity().unwrap(),
                OFFHAND_MENU_SLOT,
                Button::Left,
                |_| Ok(64),
            )
            .unwrap();
        assert_eq!(plan.carried, item(12));
        assert_eq!(plan.changed[&OFFHAND_MENU_SLOT], None);
        assert_eq!(state.slot(0, OFFHAND_MENU_SLOT), Some(&item(12)));
        assert_eq!(state.player.as_ref().unwrap().carried, Some(None));
        assert!(
            state
                .observe_storage_slot(state.active_identity().unwrap(), 5)
                .is_err()
        );
    }
    fn item(count: u32) -> Slot {
        Some(Stack {
            item: "minecraft:stone".into(),
            count,
            added: BTreeMap::new(),
            removed: BTreeSet::new(),
        })
    }
    fn menu(slot: Slot, carried: Slot) -> State {
        let mut s = State::default();
        s.begin(7, 2);
        s.admit(
            7,
            2,
            Update::Open {
                incarnation: s.incarnation + 1,
                container: 4,
                kind: "GENERIC_9X3".into(),
                title: Value::Null,
            },
        )
        .unwrap();
        let mut slots = vec![None; 63];
        slots[0] = slot;
        s.admit(
            7,
            2,
            Update::Content {
                container: 4,
                state: 19,
                slots,
                carried,
            },
        )
        .unwrap();
        s
    }
    #[test]
    fn half_pickup_and_single_placement_preserve_authority_until_server_updates() {
        let mut s = menu(item(9), None);
        let identity = s.opened_identity().unwrap();
        let p = s
            .plan_pickup(identity, 0, Button::Right, |_| Ok(64))
            .unwrap();
        assert_eq!(p.changed[&0], item(4));
        assert_eq!(p.carried, item(5));
        assert_eq!(p.state, 19);
        assert_eq!(s.slot(4, 0), Some(&item(9)));
        assert_eq!(s.opened.as_ref().unwrap().carried, Some(None));
        // A rejected action may resend the original complete state. It does not consume anything.
        s.admit(
            7,
            2,
            Update::Content {
                container: 4,
                state: 20,
                slots: {
                    let mut v = vec![None; 63];
                    v[0] = item(9);
                    v
                },
                carried: None,
            },
        )
        .unwrap();
        assert_eq!(s.slot(4, 0), Some(&item(9)));
        let s = menu(item(15), item(3));
        let p = s
            .plan_pickup(s.opened_identity().unwrap(), 0, Button::Right, |_| Ok(16))
            .unwrap();
        assert_eq!(p.changed[&0], item(16));
        assert_eq!(p.carried, item(2));
    }
    #[test]
    fn limits_and_lifetime_checks_reject_stale_or_unresolved_actions() {
        let mut s = menu(None, item(20));
        let old = s.opened_identity().unwrap();
        let p = s.plan_pickup(old, 0, Button::Left, |_| Ok(16)).unwrap();
        assert_eq!(p.changed[&0], item(16));
        assert_eq!(p.carried, item(4));
        assert!(
            s.plan_pickup(old, 0, Button::Left, |_| Err("missing facts".into()))
                .is_err()
        );
        assert!(s.plan_pickup(old, 64, Button::Left, |_| Ok(64)).is_err());
        s.admit(7, 2, Update::Close(4)).unwrap();
        s.admit(
            7,
            2,
            Update::Open {
                incarnation: s.incarnation + 1,
                container: 4,
                kind: "GENERIC_9X3".into(),
                title: Value::Null,
            },
        )
        .unwrap();
        assert!(s.plan_pickup(old, 0, Button::Left, |_| Ok(64)).is_err());
        s.begin(8, 1);
        assert!(s.plan_pickup(old, 0, Button::Left, |_| Ok(64)).is_err());
    }
}
