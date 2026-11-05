//! Native grouped item cooldowns advance in client simulation ticks, including inside menus.
use serde::Serialize;
use std::collections::BTreeMap;

// Provisional memory safety budget, not a vanilla gameplay limit. Overflow is explicit.
const MAX_BYTES: usize = 4 * 1024 * 1024;
const ENTRY_BYTES: usize = 96;
#[derive(Debug, Default, Serialize)]
pub struct Cooldowns {
    tick: u64,
    entries: BTreeMap<String, (u64, u64)>,
    bytes: usize,
}
impl Cooldowns {
    /// Native logical client ticks; gameplay never derives use duration from render frames.
    pub fn client_tick(&self) -> u64 {
        self.tick
    }

    pub fn set(&mut self, group: String, ticks: i32) -> Result<(), String> {
        if ticks <= 0 {
            self.remove(&group);
            return Ok(());
        }
        let old = self
            .entries
            .get_key_value(&group)
            .map_or(0, |(key, _)| key.capacity() + ENTRY_BYTES);
        let bytes = self.bytes - old + group.capacity() + ENTRY_BYTES;
        if bytes > MAX_BYTES {
            return Err("Native item cooldown memory budget exceeded (provisional)".into());
        }
        self.entries.remove(&group);
        self.entries
            .insert(group, (self.tick, self.tick.saturating_add(ticks as u64)));
        self.bytes = bytes;
        Ok(())
    }
    fn remove(&mut self, group: &str) {
        if let Some((key, _)) = self.entries.remove_entry(group) {
            self.bytes -= key.capacity() + ENTRY_BYTES;
        }
    }
    pub fn advance(&mut self, ticks: u64) {
        if ticks == 0 {
            return;
        }
        self.tick = self.tick.saturating_add(ticks);
        self.entries.retain(|key, &mut (_, end)| {
            if end > self.tick {
                return true;
            }
            self.bytes -= key.capacity() + ENTRY_BYTES;
            false
        });
    }
    pub fn active(&self, group: &str) -> bool {
        self.entries
            .get(group)
            .is_some_and(|&(_, end)| end > self.tick)
    }
    pub fn remaining(&self, group: &str) -> u64 {
        self.entries
            .get(group)
            .map_or(0, |&(_, end)| end.saturating_sub(self.tick))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_replacement_removal_and_expiry_use_exact_client_ticks() {
        let mut c = Cooldowns::default();
        c.set("fixture:shared".into(), 20).unwrap();
        c.advance(19);
        assert!(c.active("fixture:shared"));
        assert_eq!(c.remaining("fixture:shared"), 1);
        c.set("fixture:shared".into(), 4).unwrap();
        c.advance(3);
        assert_eq!(c.remaining("fixture:shared"), 1);
        c.advance(1);
        assert!(!c.active("fixture:shared"));
        assert_eq!(c.bytes, 0);
        c.set("fixture:other".into(), 500).unwrap();
        c.set("fixture:other".into(), 0).unwrap();
        assert_eq!(c.bytes, 0);
        c.set("fixture:other".into(), -2).unwrap();
        assert_eq!(c.bytes, 0);
    }
    #[test]
    fn oversized_admission_is_explicit_and_preserves_existing_entries() {
        let mut c = Cooldowns::default();
        c.set("fixture:kept".into(), 30).unwrap();
        assert!(c.set("x".repeat(MAX_BYTES), 10).is_err());
        assert_eq!(c.remaining("fixture:kept"), 30);
    }
}
