//! Socket-side opening identity, independent of slot contents and server state IDs.
use super::Packet;
#[derive(Debug, Default)]
pub struct Lifetime {
    serial: u64,
    active: Option<(i32, u64)>,
}
impl Lifetime {
    /// Stamps every server opening, including unsupported layouts and reused container IDs.
    pub fn observe(&mut self, packet: &Packet) -> Result<u64, &'static str> {
        match packet {
            Packet::Open { container, .. } => {
                self.serial = self
                    .serial
                    .checked_add(1)
                    .ok_or("Java menu opening identity exhausted")?;
                self.active = (*container > 0).then_some((*container, self.serial));
            }
            Packet::Close(container) if self.active.is_some_and(|(id, _)| id == *container) => {
                self.active = None
            }
            _ => {}
        }
        Ok(self.serial)
    }
    pub fn accepts(&self, container: i32, serial: u64) -> bool {
        match self.active {
            Some(identity) => identity == (container, serial),
            None => (container, serial) == (0, 0),
        }
    }
    /// World/death retirement never reuses a serial within this socket session.
    pub fn retire(&mut self) {
        self.active = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn open(id: i32) -> Packet {
        Packet::Open {
            container: id,
            kind: "GENERIC_9X3",
            title: crate::nbt::Value::String("Chest".into()),
        }
    }
    #[test]
    fn opening_replacement_close_and_world_retirement_fence_queued_actions() {
        let mut scope = Lifetime::default();
        assert!(scope.accepts(0, 0));
        let first = scope.observe(&open(7)).unwrap();
        assert!(scope.accepts(7, first));
        assert!(!scope.accepts(0, 0));
        scope.observe(&Packet::Close(6)).unwrap();
        assert!(scope.accepts(7, first));
        let second = scope.observe(&open(7)).unwrap();
        assert!(second > first);
        assert!(!scope.accepts(7, first));
        scope.retire();
        assert!(!scope.accepts(7, second));
        let third = scope.observe(&open(7)).unwrap();
        assert!(third > second);
        scope.observe(&Packet::Close(7)).unwrap();
        assert!(!scope.accepts(7, third));
        assert!(scope.accepts(0, 0));
    }
}
