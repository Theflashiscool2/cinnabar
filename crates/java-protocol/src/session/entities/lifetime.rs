//! Socket-owned spawn tokens fence attacks even when the same ID and UUID return.
use super::Packet;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Lifetimes {
    serial: u64,
    active: BTreeMap<i32, u64>,
}
impl Lifetimes {
    pub fn observe(&mut self, packet: &mut Packet) -> Result<(), &'static str> {
        match packet {
            Packet::TakeItem {
                item,
                item_lifetime,
                collector,
                collector_lifetime,
                ..
            } => {
                *item_lifetime = self.active.get(item).copied().unwrap_or(0);
                *collector_lifetime = self.active.get(collector).copied().unwrap_or(0);
            }
            Packet::Swing { lifetime, id, .. }
            | Packet::Hurt { lifetime, id }
            | Packet::Damage { lifetime, id, .. } => {
                *lifetime = self.active.get(id).copied().unwrap_or(0);
            }
            Packet::Equipment(packet) => {
                packet.lifetime = self.active.get(&packet.id).copied().unwrap_or(0);
            }
            Packet::Spawn(spawn) => {
                self.serial = self
                    .serial
                    .checked_add(1)
                    .ok_or("Java entity token exhausted")?;
                spawn.lifetime = self.serial;
                self.active.insert(spawn.id, self.serial);
            }
            Packet::Remove(ids) => {
                for id in ids {
                    self.active.remove(id);
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn accepts(&self, id: i32, token: u64) -> bool {
        token != 0 && self.active.get(&id) == Some(&token)
    }
    pub fn retire(&mut self) {
        self.active.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spawn() -> Packet {
        Packet::Spawn(super::super::Spawn {
            lifetime: 0,
            id: 4,
            uuid: [9; 16],
            kind: "minecraft:player",
            position: [0.; 3],
            velocity: [0.; 3],
            pitch: 0.,
            yaw: 0.,
            head_yaw: 0.,
            data: 0,
        })
    }
    #[test]
    fn remove_reuse_and_world_retirement_reject_old_attacks() {
        let mut owner = Lifetimes::default();
        let mut packet = spawn();
        owner.observe(&mut packet).unwrap();
        assert!(owner.accepts(4, 1));
        assert!(!owner.accepts(4, 0));
        owner.observe(&mut Packet::Remove(vec![4])).unwrap();
        assert!(!owner.accepts(4, 1));
        owner.observe(&mut spawn()).unwrap();
        assert!(!owner.accepts(4, 1));
        assert!(owner.accepts(4, 2));
        owner.retire();
        owner.observe(&mut spawn()).unwrap();
        assert!(!owner.accepts(4, 2));
        assert!(owner.accepts(4, 3));
    }
}

#[cfg(test)]
mod swing_tests {
    use super::*;
    #[test]
    fn swing_receipts_are_stamped_with_current_socket_lifetime() {
        let mut owner = Lifetimes::default();
        let mut entity = Packet::Spawn(super::super::Spawn {
            lifetime: 0,
            id: 42,
            uuid: [1; 16],
            kind: "minecraft:player",
            position: [0.; 3],
            velocity: [0.; 3],
            pitch: 0.,
            yaw: 0.,
            head_yaw: 0.,
            data: 0,
        });
        owner.observe(&mut entity).unwrap();
        let mut swing = Packet::Swing {
            lifetime: 0,
            id: 42,
            offhand: false,
            kind: 1,
            duration: 6,
        };
        owner.observe(&mut swing).unwrap();
        let Packet::Swing {
            lifetime: first, ..
        } = swing
        else {
            panic!()
        };
        assert!(owner.accepts(42, first));
        owner.observe(&mut Packet::Remove(vec![42])).unwrap();
        owner.observe(&mut entity).unwrap();
        assert!(!owner.accepts(42, first));
        owner.observe(&mut swing).unwrap();
        let Packet::Swing { lifetime: next, .. } = swing else {
            panic!()
        };
        assert!(owner.accepts(42, next));
        assert_ne!(first, next);
    }
}

#[cfg(test)]
mod hurt_tests {
    use super::*;
    #[test]
    fn hurt_notifications_use_the_socket_lifetime_and_retire_on_removal() {
        let mut owner = Lifetimes::default();
        let mut hurt = Packet::Hurt { lifetime: 7, id: 4 };
        owner.observe(&mut hurt).unwrap();
        assert!(matches!(hurt, Packet::Hurt { lifetime: 0, .. }));
        let spawn = || {
            Packet::Spawn(super::super::Spawn {
                lifetime: 0,
                id: 4,
                uuid: [1; 16],
                kind: "minecraft:player",
                position: [0.; 3],
                velocity: [0.; 3],
                pitch: 0.,
                yaw: 0.,
                head_yaw: 0.,
                data: 0,
            })
        };
        owner.observe(&mut spawn()).unwrap();
        owner.observe(&mut hurt).unwrap();
        assert!(matches!(hurt, Packet::Hurt { lifetime: 1, .. }));
        owner.observe(&mut Packet::Remove(vec![4])).unwrap();
        owner.observe(&mut hurt).unwrap();
        assert!(matches!(hurt, Packet::Hurt { lifetime: 0, .. }));
        owner.observe(&mut spawn()).unwrap();
        owner.observe(&mut hurt).unwrap();
        assert!(matches!(hurt, Packet::Hurt { lifetime: 2, .. }));
    }
}
