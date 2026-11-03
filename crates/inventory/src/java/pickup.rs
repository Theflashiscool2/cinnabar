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
        if receipt.written && receipt.closed {
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
    /// Computes vanilla pickup metadata without mutating slots/cursor or declaring acceptance.
    /// Specialized slots (crafting, armor, outputs) require their own native placement rules.
    pub fn plan_pickup(
        &self,
        identity: MenuIdentity,
        slot: usize,
        button: Button,
        mut max_stack: impl FnMut(&Stack) -> Result<u32, String>,
    ) -> Result<Plan, String> {
        if self.active_identity() != Some(identity) || self.error.is_some() {
            return Err("Java container lifetime is no longer current".into());
        }
        let menu = self.menu(identity.container).unwrap();
        if (menu.container == 0 && !(9..=44).contains(&slot))
            || (menu.container != 0 && !menu.kind.starts_with("GENERIC_9X"))
        {
            return Err("Java specialized slot rules not implemented".into());
        }
        let state = menu.state.ok_or("Java container contents are not ready")?;
        let current = self
            .slot(menu.container, slot)
            .ok_or("Java slot is unknown")?;
        let carried = menu
            .carried
            .as_ref()
            .ok_or("Java carried stack is unknown")?;
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
                if same_components(item, held) {
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
fn same_components(a: &Stack, b: &Stack) -> bool {
    a.item == b.item && a.added == b.added && a.removed == b.removed
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
