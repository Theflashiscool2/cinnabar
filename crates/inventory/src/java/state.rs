//! Applies ordered server facts to scoped inventory and container ownership.
use super::*;
impl State {
    /// A play login reuses the local player's storage, but retires foreign menus and receipts.
    pub fn rebind_player(&mut self, session: u64, epoch: u64) {
        if self.session != session || self.player.is_none() {
            self.begin(session, epoch);
            return;
        }
        let storage = std::mem::take(&mut self.storage);
        let selected = self.selected;
        self.begin(session, epoch);
        self.storage = storage;
        self.selected = selected;
    }
    /// A replacement player has no admitted inventory, even when the server keeps metadata.
    pub fn begin(&mut self, session: u64, epoch: u64) {
        *self = Self {
            session,
            epoch,
            selected: Some(0),
            storage: vec![Cell::Unknown; PLAYER_STORAGE_SIZE],
            player: Some(Menu {
                container: 0,
                incarnation: 0,
                kind: "player".into(),
                title: Value::Null,
                state: None,
                slots: (0..PLAYER_MENU_SIZE)
                    .map(|slot| {
                        player_storage_slot(slot)
                            .map_or(Binding::Owned(Cell::Unknown), Binding::Player)
                    })
                    .collect(),
                carried: None,
                properties: BTreeMap::new(),
            }),
            ..Self::default()
        };
    }
    /// Outer None is unknown; Some(None) is an authoritative empty main hand.
    pub fn held(&self) -> Option<&Slot> {
        if self.error.is_some() {
            return None;
        }
        self.storage.get(self.selected?)?.value()
    }
    pub fn accepts(&self, session: u64, epoch: u64) -> bool {
        (self.session, self.epoch) == (session, epoch) && self.player.is_some()
    }
    pub fn admit(&mut self, session: u64, epoch: u64, update: Update) -> Result<bool, String> {
        if !self.accepts(session, epoch) {
            return Ok(false);
        }
        let result = self.apply(update);
        match &result {
            Ok(true) => self.revision = self.revision.saturating_add(1),
            Err(error) => self.error = Some(error.clone()),
            Ok(false) => {}
        }
        result
    }
    /// Applies a local choice only after serialization, in the same event order as server facts.
    pub fn selection_sent(&mut self, session: u64, epoch: u64, sequence: u64, slot: usize) -> bool {
        if !self.accepts(session, epoch)
            || self.selection_receipt.is_some_and(|old| sequence <= old)
        {
            return false;
        }
        if self.admit(session, epoch, Update::Selected(slot)) != Ok(true) {
            return false;
        }
        self.selection_receipt = Some(sequence);
        true
    }
    pub fn menu(&self, id: i32) -> Option<&Menu> {
        if id == 0 {
            self.player.as_ref()
        } else {
            self.opened.as_ref().filter(|m| m.container == id)
        }
    }
    /// Resolves a menu view against the one authoritative player storage.
    pub fn slot(&self, id: i32, index: usize) -> Option<&Slot> {
        match self.menu(id)?.slots.get(index)? {
            Binding::Owned(value) => value.value(),
            Binding::Player(index) => self.storage.get(*index)?.value(),
        }
    }
    fn set_slot(&mut self, id: i32, index: usize, stack: Slot) {
        let storage = match &self.menu(id).unwrap().slots[index] {
            Binding::Player(storage) => Some(*storage),
            Binding::Owned(_) => None,
        };
        if let Some(storage) = storage {
            self.storage[storage] = Cell::Known(stack);
        } else {
            self.menu_mut(id).unwrap().slots[index] = Binding::Owned(Cell::Known(stack));
        }
    }
    fn menu_mut(&mut self, id: i32) -> Option<&mut Menu> {
        if id == 0 {
            self.player.as_mut()
        } else {
            self.opened.as_mut().filter(|m| m.container == id)
        }
    }
    fn apply(&mut self, update: Update) -> Result<bool, String> {
        match update {
            Update::Selected(slot) => {
                if slot >= HOTBAR_SIZE {
                    return Err("Java selected hotbar slot is out of range".into());
                }
                self.selected = Some(slot);
            }
            Update::PlayerSlot { slot, stack } => {
                if slot >= self.storage.len() {
                    return Err("Java player inventory slot is out of range".into());
                }
                self.storage[slot] = Cell::Known(stack);
            }
            Update::Content {
                container,
                state,
                slots,
                carried,
            } => {
                let Some(menu) = self.menu_mut(container) else {
                    return Ok(false);
                };
                if slots.len() != menu.slots.len() {
                    return Err("Java container content length disagrees with its layout".into());
                }
                if state < 0 {
                    return Err("negative Java container state".into());
                }
                menu.state = Some(state);
                menu.carried = Some(carried);
                for (slot, stack) in slots.into_iter().enumerate() {
                    self.set_slot(container, slot, stack);
                }
                if container == 0 {
                    self.error = None;
                }
            }
            Update::Slot {
                container,
                state,
                slot,
                stack,
            } => {
                let Some(menu) = self.menu_mut(container) else {
                    return Ok(false);
                };
                if slot >= menu.slots.len() {
                    return Err("Java container slot is out of range".into());
                }
                if state < 0 {
                    return Err("negative Java container state".into());
                }
                // State IDs are transaction snapshots, not sequence numbers to sort or reject.
                menu.state = Some(state);
                self.set_slot(container, slot, stack);
            }
            Update::Cursor(stack) => {
                let menu = self.opened.as_mut().or(self.player.as_mut()).unwrap();
                menu.carried = Some(stack);
            }
            Update::Open {
                container,
                kind,
                title,
            } => {
                if container <= 0 {
                    return Err("Java external menu needs a positive container ID".into());
                }
                self.opened = None;
                let rows = kind
                    .strip_prefix("GENERIC_9X")
                    .and_then(|s| s.parse::<usize>().ok())
                    .filter(|r| (1..=6).contains(r));
                let Some(rows) = rows else {
                    return Err(format!("Native Java menu {kind} is not implemented"));
                };
                self.incarnation = self
                    .incarnation
                    .checked_add(1)
                    .ok_or("Java menu incarnation exhausted")?;
                let start = rows * HOTBAR_SIZE;
                self.opened = Some(Menu {
                    container,
                    incarnation: self.incarnation,
                    kind,
                    title,
                    state: None,
                    slots: (0..start)
                        .map(|_| Binding::Owned(Cell::Unknown))
                        .chain(
                            (HOTBAR_SIZE..HOTBAR_START)
                                .chain(0..HOTBAR_SIZE)
                                .map(Binding::Player),
                        )
                        .collect(),
                    carried: None,
                    properties: BTreeMap::new(),
                });
            }
            Update::Close(container) => {
                if self
                    .opened
                    .as_ref()
                    .is_none_or(|m| m.container != container)
                {
                    return Ok(false);
                }
                self.opened = None;
            }
            Update::Property {
                container,
                property,
                value,
            } => {
                let Some(menu) = self.menu_mut(container) else {
                    return Ok(false);
                };
                menu.properties.insert(property, value);
            }
        }
        Ok(true)
    }
}
