//! Java inventory authority. Unknown slots are distinct from server-confirmed empty stacks.
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

// Pinned InventoryMenu/Inventory slot ownership, verified by the native fixture vectors.
pub const HOTBAR_SIZE: usize = 9;
pub const PLAYER_MENU_SIZE: usize = 46;
pub const PLAYER_STORAGE_SIZE: usize = 43;
pub const HOTBAR_START: usize = 36;
pub const OFFHAND_MENU_SLOT: usize = 45;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stack {
    pub item: String,
    pub count: u32,
    /// Schema-named component overrides, not resolved default components.
    pub added: BTreeMap<String, Value>,
    pub removed: BTreeSet<String>,
}
pub type Slot = Option<Stack>;
#[derive(Clone, Debug)]
pub enum Update {
    Content {
        container: i32,
        state: i32,
        slots: Vec<Slot>,
        carried: Slot,
    },
    Slot {
        container: i32,
        state: i32,
        slot: usize,
        stack: Slot,
    },
    PlayerSlot {
        slot: usize,
        stack: Slot,
    },
    Cursor(Slot),
    Selected(usize),
    Open {
        container: i32,
        kind: String,
        title: Value,
    },
    Close(i32),
    Property {
        container: i32,
        property: i16,
        value: i16,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize)]
enum Cell {
    Unknown,
    Known(Slot),
}
impl Cell {
    fn value(&self) -> Option<&Slot> {
        match self {
            Self::Unknown => None,
            Self::Known(value) => Some(value),
        }
    }
}
#[derive(Debug, Serialize)]
enum Binding {
    Owned(Cell),
    Player(usize),
}
#[derive(Debug, Serialize)]
pub struct Menu {
    pub container: i32,
    pub incarnation: u64,
    pub kind: String,
    pub title: Value,
    pub state: Option<i32>,
    slots: Vec<Binding>,
    pub carried: Option<Slot>,
    pub properties: BTreeMap<i16, i16>,
}
#[derive(Debug, Default, Serialize)]
pub struct State {
    session: u64,
    epoch: u64,
    incarnation: u64,
    pub revision: u64,
    pub selected: Option<usize>,
    /// Last local selection serialized by the socket; server selections remain ordered facts.
    pub selection_receipt: Option<u64>,
    storage: Vec<Cell>,
    pub player: Option<Menu>,
    pub opened: Option<Menu>,
    pub error: Option<String>,
}
mod state;

fn player_storage_slot(menu_slot: usize) -> Option<usize> {
    match menu_slot {
        5..9 => Some(44 - menu_slot),
        HOTBAR_SIZE..HOTBAR_START => Some(menu_slot),
        HOTBAR_START..OFFHAND_MENU_SLOT => Some(menu_slot - HOTBAR_START),
        OFFHAND_MENU_SLOT => Some(40),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod world_tests;
