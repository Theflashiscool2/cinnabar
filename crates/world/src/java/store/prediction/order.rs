//! Native FastUtil 8.5.18 long-map key order, used when rollback order is observable.
//! Values stay in the terrain prediction owner; this index contains only cell keys.
#[derive(Debug)]
pub(in crate::java::store) struct Order {
    slots: Vec<Option<[i32; 3]>>,
    zero: bool,
    len: usize,
}
impl Default for Order {
    fn default() -> Self {
        Self {
            slots: vec![None; 32],
            zero: false,
            len: 0,
        }
    }
}
fn key([x, y, z]: [i32; 3]) -> u64 {
    ((x as u64 & 0x3ff_ffff) << 38) | ((z as u64 & 0x3ff_ffff) << 12) | (y as u64 & 0xfff)
}
fn hash(position: [i32; 3]) -> usize {
    let mixed = key(position).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mixed = mixed ^ (mixed >> 32);
    (mixed ^ (mixed >> 16)) as usize
}
impl Order {
    pub(super) fn insert(&mut self, position: [i32; 3]) {
        if key(position) == 0 {
            if self.zero {
                return;
            }
            self.zero = true;
        } else {
            let mask = self.slots.len() - 1;
            let mut slot = hash(position) & mask;
            while let Some(existing) = self.slots[slot] {
                if existing == position {
                    return;
                }
                slot = (slot + 1) & mask;
            }
            self.slots[slot] = Some(position);
        }
        self.len += 1;
        if self.len > self.slots.len() * 3 / 4 {
            let capacity = self.slots.len() * 2;
            let old = std::mem::replace(&mut self.slots, vec![None; capacity]);
            let mask = capacity - 1;
            for position in old.into_iter().rev().flatten() {
                let mut slot = hash(position) & mask;
                while self.slots[slot].is_some() {
                    slot = (slot + 1) & mask;
                }
                self.slots[slot] = Some(position);
            }
        }
    }
    pub(super) fn positions(&self) -> impl Iterator<Item = [i32; 3]> + '_ {
        self.zero
            .then_some([0; 3])
            .into_iter()
            .chain(self.slots.iter().rev().filter_map(|p| *p))
    }
    pub(super) fn remove(&mut self, position: [i32; 3]) {
        if key(position) == 0 {
            if self.zero {
                self.zero = false;
                self.len -= 1;
            }
            return;
        }
        let mask = self.slots.len() - 1;
        let mut hole = hash(position) & mask;
        loop {
            match self.slots[hole] {
                Some(existing) if existing == position => break,
                Some(_) => hole = (hole + 1) & mask,
                None => return,
            }
        }
        self.len -= 1;
        loop {
            let mut scan = (hole + 1) & mask;
            loop {
                let Some(next) = self.slots[scan] else {
                    self.slots[hole] = None;
                    return;
                };
                let home = hash(next) & mask;
                let crosses_hole = if hole <= scan {
                    home <= hole || home > scan
                } else {
                    home <= hole && home > scan
                };
                if crosses_hole {
                    break;
                }
                scan = (scan + 1) & mask;
            }
            self.slots[hole] = self.slots[scan];
            hole = scan;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn position(id: i32) -> [i32; 3] {
        if id == 0 {
            return [0; 3];
        }
        [(id * 17) % 61 - 30, -64 + id % 16, (id * 29) % 73 - 36]
    }
    #[test]
    fn native_key_order_survives_rehash_iterator_removal_and_later_insertions() {
        // NativePredictionOrderFixture.java, fastutil-8.5.18.jar from the pinned runtime.
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("native-order.json")).unwrap();
        let expected = |name: &str| -> Vec<[i32; 3]> {
            fixture[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| position(id.as_i64().unwrap() as i32))
                .collect()
        };
        let mut order = Order::default();
        for id in 0..80 {
            order.insert(position(id));
        }
        assert_eq!(order.positions().collect::<Vec<_>>(), expected("before"));
        let removed: Vec<_> = order
            .positions()
            .filter(|p| (0..80).find(|id| position(*id) == *p).unwrap() % 3 == 0)
            .collect();
        assert_eq!(removed, expected("removed"));
        for position in removed {
            order.remove(position);
        }
        assert_eq!(order.positions().collect::<Vec<_>>(), expected("after"));
        for id in 80..110 {
            order.insert(position(id));
        }
        assert_eq!(order.positions().collect::<Vec<_>>(), expected("extended"));
    }
}
