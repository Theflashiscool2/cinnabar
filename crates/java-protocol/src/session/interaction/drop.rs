//! Socket-side admission fences. These stamps never claim server acceptance.
use super::super::{hud::GameMode, inventory::Packet};
#[derive(Default)]
pub struct Fence {
    pub revision: u64,
    pub selected: u8,
    pub spectator: bool,
}
impl Fence {
    pub fn observe(&mut self, packet: &Packet) -> Result<(), &'static str> {
        if packet.changes_items() {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or("Java inventory wire revision overflow")?;
        }
        if let Packet::Selected(slot) = packet {
            self.selected = u8::try_from(*slot)
                .ok()
                .filter(|v| *v < 9)
                .ok_or("Java selected slot out of range")?;
        }
        Ok(())
    }
    pub fn mode(&mut self, mode: GameMode) {
        self.spectator = mode == GameMode::Spectator;
    }
    pub fn accepts(&self, revision: u64, slot: u8) -> bool {
        !self.spectator && slot < 9 && slot == self.selected && revision == self.revision
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incoming_items_selection_and_spectator_fence_queued_drops() {
        let mut f = Fence::default();
        assert!(f.accepts(0, 0));
        f.observe(&Packet::PlayerSlot {
            slot: 0,
            stack: None,
        })
        .unwrap();
        assert!(!f.accepts(0, 0));
        assert!(f.accepts(1, 0));
        f.observe(&Packet::Selected(2)).unwrap();
        assert!(!f.accepts(1, 0));
        assert!(f.accepts(1, 2));
        f.mode(GameMode::Spectator);
        assert!(!f.accepts(1, 2));
        f.mode(GameMode::Survival);
        f.observe(&Packet::Cooldown {
            group: "fixture".into(),
            ticks: 4,
        })
        .unwrap();
        assert!(f.accepts(1, 2));
        assert!(f.observe(&Packet::Selected(9)).is_err());
    }
}
