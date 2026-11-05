//! One transport-owned neutral cancellation; never a second input/action owner.
//! ServerGamePacketListener ignores PlayerAction before PlayerLoaded. Retain one
//! cancellation until the real loaded packet is written, and retire it on epoch changes.
#[derive(Default)]
pub(crate) struct Pending {
    epoch: Option<u64>,
}
impl Pending {
    pub(crate) fn hold(&mut self, requested: u64, current: u64) {
        if requested == current {
            self.epoch = Some(current);
        }
    }
    pub(crate) fn clear(&mut self) {
        self.epoch = None;
    }
    pub(crate) fn take_ready(&mut self, current: u64, loaded: Option<u64>, dead: bool) -> bool {
        if dead || self.epoch.is_some_and(|epoch| epoch != current) {
            self.clear();
            return false;
        }
        if self.epoch == Some(current) && loaded == Some(current) {
            self.clear();
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_waits_for_actual_loaded_write_and_coalesces_without_replaying() {
        let mut p = Pending::default();
        p.hold(7, 7);
        p.hold(7, 7);
        assert!(!p.take_ready(7, None, false));
        assert!(!p.take_ready(7, Some(6), false));
        assert!(p.take_ready(7, Some(7), false));
        assert!(!p.take_ready(7, Some(7), false));
    }
    #[test]
    fn stale_requests_and_new_world_death_or_reconfiguration_cannot_reuse_cancellation() {
        let mut p = Pending::default();
        p.hold(8, 8);
        p.hold(7, 8);
        assert!(p.take_ready(8, Some(8), false));
        p.hold(8, 8);
        assert!(!p.take_ready(9, None, false));
        assert!(!p.take_ready(9, Some(9), false));
        p.hold(9, 9);
        assert!(!p.take_ready(9, Some(9), true));
        assert!(!p.take_ready(9, Some(9), false));
        p.hold(9, 9);
        p.clear();
        assert!(!p.take_ready(9, Some(9), false));
    }
}
