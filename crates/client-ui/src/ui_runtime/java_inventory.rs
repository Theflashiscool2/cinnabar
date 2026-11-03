//! Native inventory values reach the shared HUD without a Bedrock registry or ledger.
use super::{UiRuntime, presentation::UiPresentationRuntime};
use inventory::java::{HOTBAR_START, OFFHAND_MENU_SLOT, Slot};
use ui::IconRef;

#[derive(Clone, Debug, Default)]
pub(super) struct Projection {
    key: Option<(u64, u64, u64)>,
    hotbar: [Option<Slot>; 9],
    offhand: Option<Slot>,
    player: Vec<Option<Slot>>,
    opened: Option<MenuView>,
    player_carried: Option<Slot>,
    identity: Option<inventory::java::pickup::MenuIdentity>,
}

/// Artwork and counts admitted for a native frame. Missing artwork never hides counts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudItems {
    pub hotbar: [HudItem; 9],
    pub offhand: HudItem,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudItem {
    pub known: bool,
    pub count: u32,
    pub icon: Option<IconRef>,
    pub durability: Option<f64>,
}
#[derive(Clone, Debug)]
struct MenuView {
    identity: inventory::java::pickup::MenuIdentity,
    kind: String,
    title: serde_json::Value,
    slots: Vec<Option<Slot>>,
    carried: Option<Slot>,
}
impl UiRuntime {
    /// Asset-worker output may publish only into the matching active native world.
    pub fn publish_java_item_names(
        &mut self,
        session: u64,
        epoch: u64,
        names: std::sync::Arc<assets::registry::java::language::Language>,
    ) -> bool {
        if !self
            .java_hud()
            .is_some_and(|hud| (hud.session, hud.epoch) == (session, epoch))
        {
            return false;
        }
        self.session.java_item_names = Some(names);
        true
    }
    /// Copies only changed authoritative native slots, rejecting retired world/player scopes.
    pub fn publish_java_inventory(&mut self, world: &client_world::java::WorldState) -> bool {
        let Some(hud) = self.java_hud() else {
            return false;
        };
        if hud.session != world.generation()
            || hud.session != self.session.session_id
            || !world.inventory.accepts(hud.session, hud.epoch)
        {
            return false;
        }
        let key = (hud.session, hud.epoch, world.inventory.revision);
        if self.session.java_inventory.key == Some(key) {
            return true;
        }
        let old_menu = self
            .session
            .java_inventory
            .opened
            .as_ref()
            .map(|m| m.identity);
        let new_menu = world.inventory.opened_identity();
        if old_menu != new_menu {
            self.session.inventory_open = new_menu.is_some();
            self.session.chat_focused = false;
            self.session.screen.pointer.reset();
            self.session.screen.hover = None;
            self.session.screen.container_close_armed = false;
            self.session.inventory_pointer_gui = None;
            self.session.java_inventory_inputs.clear();
            self.session.java_inventory_error = None;
        }
        self.session.java_inventory = Projection {
            key: Some(key),
            hotbar: std::array::from_fn(|i| world.inventory.slot(0, HOTBAR_START + i).cloned()),
            offhand: world.inventory.slot(0, OFFHAND_MENU_SLOT).cloned(),
            player: (0..inventory::java::PLAYER_MENU_SIZE)
                .map(|i| world.inventory.slot(0, i).cloned())
                .collect(),
            player_carried: world
                .inventory
                .player
                .as_ref()
                .and_then(|m| m.carried.clone()),
            identity: world.inventory.active_identity(),
            opened: world.inventory.opened.as_ref().and_then(|menu| {
                Some(MenuView {
                    identity: world.inventory.opened_identity()?,
                    kind: menu.kind.clone(),
                    title: menu.title.clone(),
                    slots: (0..menu.len())
                        .map(|i| world.inventory.slot(menu.container, i).cloned())
                        .collect(),
                    carried: menu.carried.clone(),
                })
            }),
        };
        true
    }
    /// Only implemented layouts are offered; specialized station rules remain explicit.
    pub fn java_container_slots(&self) -> Option<Option<usize>> {
        self.java_hud()?;
        match &self.session.java_inventory.opened {
            None => Some(None),
            Some(menu) => match menu.kind.as_str() {
                "GENERIC_9X3" => Some(Some(27)),
                "GENERIC_9X6" => Some(Some(54)),
                _ => None,
            },
        }
    }
    pub(super) fn java_container_title(&self) -> Option<String> {
        self.java_hud()?;
        self.session.java_inventory.opened.as_ref().map(|m| {
            component_title_with(&m.title, &|key| {
                self.session
                    .java_item_names
                    .as_deref()
                    .and_then(|lang| lang.get(key))
                    .map(str::to_owned)
                    .or_else(|| self.translation(key).map(|s| s.to_string()))
            })
        })
    }
    pub(super) fn java_container_frame(
        &self,
        artwork: &UiPresentationRuntime,
    ) -> Option<ui_screens::containers::NativeInventory> {
        self.java_hud()?;
        let projection = &self.session.java_inventory;
        let cells = |slots: &[Option<Slot>]| {
            slots
                .iter()
                .map(|s| cell(s, artwork, self.session.java_item_names.as_deref()))
                .collect()
        };
        Some(ui_screens::containers::NativeInventory {
            player: cells(&projection.player),
            container: projection.opened.as_ref().map_or_else(Vec::new, |m| {
                cells(&m.slots[..m.slots.len().saturating_sub(36)])
            }),
            cursor: cell(
                &projection
                    .opened
                    .as_ref()
                    .map_or_else(|| projection.player_carried.clone(), |m| m.carried.clone()),
                artwork,
                self.session.java_item_names.as_deref(),
            ),
        })
    }
    pub fn java_inventory_holding(&self) -> bool {
        let p = &self.session.java_inventory;
        p.opened
            .as_ref()
            .map_or(p.player_carried.as_ref(), |m| m.carried.as_ref())
            .is_some_and(|s| s.is_some())
    }
    pub(super) fn java_inventory_items(&self, artwork: &UiPresentationRuntime) -> Option<HudItems> {
        self.java_hud()?;
        let projection = &self.session.java_inventory;
        Some(HudItems {
            hotbar: std::array::from_fn(|i| item(&projection.hotbar[i], artwork)),
            offhand: item(&projection.offhand, artwork),
        })
    }
}
fn item(slot: &Option<Slot>, artwork: &UiPresentationRuntime) -> HudItem {
    let Some(slot) = slot else {
        return HudItem::default();
    };
    let Some(stack) = slot else {
        return HudItem {
            known: true,
            ..Default::default()
        };
    };
    // The shared default atlas is keyed by names, not either edition's numeric IDs.
    // Custom Java models need their own resolver; don't misrepresent an overridden model.
    let ordinary = !stack.removed.contains("minecraft:item_model")
        && stack
            .added
            .get("minecraft:item_model")
            .is_none_or(|model| model.as_str() == Some(stack.item.as_str()));
    let icon = ordinary
        .then(|| artwork.item_icon(&stack.item, 0))
        .flatten();
    let durability = stack
        .added
        .get("minecraft:max_damage")
        .and_then(|v| v.as_u64())
        .filter(|max| *max > 0)
        .and_then(|max| {
            stack
                .added
                .get("minecraft:damage")
                .and_then(|v| v.as_u64())
                .map(|damage| (max.saturating_sub(damage) as f64 / max as f64).clamp(0.0, 1.0))
        });
    let glint = stack
        .added
        .get("minecraft:enchantment_glint_override")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| {
            stack
                .added
                .get("minecraft:enchantments")
                .is_some_and(|v| v.as_object().is_some_and(|v| !v.is_empty()))
        });
    HudItem {
        known: true,
        count: stack.count,
        icon: icon.map(|i| i.with_glint(glint)),
        durability,
    }
}
impl HudItems {
    /// Values/indices are compared without allocating or fabricating an item ID.
    pub(crate) fn matches(
        &self,
        model: &json_ui::HudModel,
        icons: &[IconRef],
        selected: Option<u8>,
    ) -> bool {
        model.hotbar.len() == 9
            && self
                .hotbar
                .iter()
                .zip(&model.hotbar)
                .enumerate()
                .all(|(i, (item, cell))| item.matches(cell, icons, selected == Some(i as u8)))
            && match (&model.offhand, self.offhand.count) {
                (None, 0) => true,
                (Some(cell), n) if n > 0 => self.offhand.matches(cell, icons, false),
                _ => false,
            }
    }
    pub(crate) fn bind(
        &self,
        icons: &mut Vec<IconRef>,
        selected: Option<u8>,
    ) -> (Vec<json_ui::HudSlot>, Option<json_ui::HudSlot>) {
        let hotbar = self
            .hotbar
            .iter()
            .enumerate()
            .map(|(i, item)| item.bind(icons, selected == Some(i as u8)))
            .collect();
        let offhand = (self.offhand.count > 0).then(|| self.offhand.bind(icons, false));
        (hotbar, offhand)
    }
}
impl HudItem {
    fn matches(&self, cell: &json_ui::HudSlot, icons: &[IconRef], selected: bool) -> bool {
        cell.count == self.count
            && cell.id_aux.is_none()
            && cell.selected == selected
            && cell.durability == self.durability
            && cell.icon.and_then(|i| icons.get(i).copied()) == self.icon
    }
    fn bind(&self, icons: &mut Vec<IconRef>, selected: bool) -> json_ui::HudSlot {
        let icon = self.icon.map(|icon| {
            icons.push(icon);
            icons.len() - 1
        });
        json_ui::HudSlot {
            icon,
            id_aux: None,
            count: self.count,
            selected,
            durability: self.durability,
        }
    }
}
#[cfg(test)]
mod tests;

fn cell(
    slot: &Option<Slot>,
    artwork: &UiPresentationRuntime,
    names: Option<&assets::registry::java::language::Language>,
) -> ui_screens::containers::NativeCell {
    let visual = item(slot, artwork);
    let name = slot
        .as_ref()
        .and_then(|s| s.as_ref())
        .map_or_else(String::new, |stack| stack_name(stack, names));
    ui_screens::containers::NativeCell {
        known: visual.known,
        count: visual.count,
        icon: visual.icon,
        durability: visual.durability.map(|v| v as f32),
        name,
    }
}
fn stack_name(
    stack: &inventory::java::Stack,
    names: Option<&assets::registry::java::language::Language>,
) -> String {
    let component = stack
        .added
        .get("minecraft:custom_name")
        .or_else(|| stack.added.get("minecraft:item_name"));
    if component.is_none() && stack.removed.contains("minecraft:item_name") {
        return String::new(); // Item.getName uses CommonComponents.EMPTY when ITEM_NAME is absent.
    }
    component
        .or_else(|| names.and_then(|lang| lang.item(&stack.item)))
        .map(|component| {
            component_title_with(component, &|key| {
                names.and_then(|lang| lang.get(key)).map(str::to_owned)
            })
        })
        .unwrap_or_else(|| stack.item.clone())
}
#[cfg(test)]
fn component_title(value: &serde_json::Value) -> String {
    component_title_with(value, &|_| None)
}
fn component_title_with(
    value: &serde_json::Value,
    translate: &impl Fn(&str) -> Option<String>,
) -> String {
    fn append(
        v: &serde_json::Value,
        out: &mut String,
        budget: &mut usize,
        depth: usize,
        translate: &impl Fn(&str) -> Option<String>,
    ) {
        if depth > 32 || *budget == 0 {
            return;
        }
        *budget -= 1;
        let v = v
            .get("String")
            .or_else(|| v.get("Compound"))
            .or_else(|| v.get("List"))
            .unwrap_or(v);
        if let Some(s) = v.as_str() {
            for c in s.chars().take(512usize.saturating_sub(out.chars().count())) {
                out.push(c);
            }
        } else if let Some(parts) = v.as_array() {
            for part in parts.iter().take(256) {
                append(part, out, budget, depth + 1, translate);
            }
        } else if let Some(map) = v.as_object() {
            if let Some(literal) = map.get("text") {
                append(literal, out, budget, depth + 1, translate);
            } else if let Some(key) = map.get("translate") {
                let key = key.get("String").unwrap_or(key).as_str().unwrap_or("");
                if let Some(text) = translate(key) {
                    append(
                        &serde_json::Value::String(text),
                        out,
                        budget,
                        depth + 1,
                        translate,
                    );
                } else if let Some(fallback) = map.get("fallback") {
                    append(fallback, out, budget, depth + 1, translate);
                } else {
                    append(
                        &serde_json::Value::String(key.into()),
                        out,
                        budget,
                        depth + 1,
                        translate,
                    );
                }
            }
            if let Some(extra) = map.get("extra") {
                append(extra, out, budget, depth + 1, translate);
            }
        }
    }
    let mut out = String::new();
    append(value, &mut out, &mut 256, 0, translate);
    out
}

/// Bounded, scope-stamped user intents. The application sends them through native transport.
#[derive(Clone, Debug)]
pub enum Intent {
    Pickup {
        slot: usize,
        button: inventory::java::pickup::Button,
    },
    QuickMove {
        slot: usize,
    },
    Collect {
        slot: usize,
    },
    Swap {
        slot: usize,
        hotbar: u8,
    },
    Throw {
        slot: usize,
        whole_stack: bool,
    },
    Close,
}
#[derive(Clone, Debug)]
pub struct Input {
    pub identity: inventory::java::pickup::MenuIdentity,
    pub sequence: u64,
    pub intent: Intent,
}
impl UiRuntime {
    fn queue_java_inventory(&mut self, intent: Intent) {
        let Some(identity) = self.session.java_inventory.identity else {
            self.session.java_inventory_error =
                Some("Java inventory contents are not ready".into());
            return;
        };
        if self.session.java_inventory_inputs.len() >= 32 {
            self.session.java_inventory_error = Some("Java inventory input queue is full".into());
            return;
        }
        self.session.java_inventory_sequence =
            self.session.java_inventory_sequence.saturating_add(1);
        self.session.java_inventory_inputs.push_back(Input {
            identity,
            sequence: self.session.java_inventory_sequence,
            intent,
        });
    }
    pub fn next_java_inventory_input(&self) -> Option<&Input> {
        self.session.java_inventory_inputs.front()
    }
    pub fn finish_java_inventory_input(&mut self, error: Option<String>) {
        self.session.java_inventory_inputs.pop_front();
        self.session.java_inventory_error = error;
    }
    pub fn java_inventory_error(&self) -> Option<&str> {
        self.session.java_inventory_error.as_deref()
    }
    pub(super) fn queue_java_container_close(&mut self) {
        self.queue_java_inventory(Intent::Close);
    }
    pub(super) fn java_pointer_action(&mut self, action: super::inventory_drag::PointerAction) {
        use super::inventory_drag::PointerAction;
        use inventory::java::pickup::Button;
        let (hit, button) = match action {
            PointerAction::Click(hit) => (hit, Some(Button::Left)),
            PointerAction::SecondaryClick(hit) => (hit, Some(Button::Right)),
            PointerAction::QuickMove(hit) => (hit, None),
            PointerAction::Gather => {
                if let Some(slot) = self
                    .screen_state()
                    .hover
                    .and_then(|hit| self.java_menu_slot(hit))
                {
                    self.queue_java_inventory(Intent::Collect { slot });
                }
                return;
            }
            PointerAction::EndDistribute => return,
            _ => {
                self.session.java_inventory_error =
                    Some("This native Java inventory gesture is not implemented".into());
                return;
            }
        };
        if let Some(slot) = self.java_menu_slot(hit) {
            self.queue_java_inventory(match button {
                Some(button) => Intent::Pickup { slot, button },
                None => Intent::QuickMove { slot },
            });
        }
    }
    pub(super) fn java_keyboard_action(
        &mut self,
        hit: Option<ui_screens::container_actions::InventoryCellHit>,
        hotbar: Option<u8>,
        drop: bool,
        whole_stack: bool,
    ) {
        if self.screen_state().text_focused() {
            return;
        }
        let Some(slot) = hit.and_then(|hit| self.java_menu_slot(hit)) else {
            return;
        };
        if let Some(hotbar) = hotbar.filter(|n| *n < 9) {
            self.queue_java_inventory(Intent::Swap { slot, hotbar });
        } else if drop {
            if self.java_inventory_holding() {
                self.session.java_inventory_error =
                    Some("Place the carried stack before dropping a hovered item".into());
            } else {
                self.queue_java_inventory(Intent::Throw { slot, whole_stack });
            }
        }
    }
    fn java_menu_slot(
        &mut self,
        hit: ui_screens::container_actions::InventoryCellHit,
    ) -> Option<usize> {
        use ui_screens::container_actions::InventoryCellHit;
        let slots = self.java_container_slots()?;
        let slot = match (hit, slots) {
            (InventoryCellHit::Offhand, None) => OFFHAND_MENU_SLOT,
            (InventoryCellHit::Player(slot), None) if slot < 9 => 36 + usize::from(slot),
            (InventoryCellHit::Player(slot), None) if slot < 36 => usize::from(slot),
            (InventoryCellHit::Player(slot), Some(start)) if slot < 9 => {
                start + 27 + usize::from(slot)
            }
            (InventoryCellHit::Player(slot), Some(start)) if slot < 36 => {
                start + usize::from(slot) - 9
            }
            (InventoryCellHit::Storage(slot), Some(start)) if usize::from(slot) < start => {
                usize::from(slot)
            }
            _ => {
                self.session.java_inventory_error =
                    Some("This native Java slot requires specialized rules".into());
                return None;
            }
        };
        Some(slot)
    }
}
