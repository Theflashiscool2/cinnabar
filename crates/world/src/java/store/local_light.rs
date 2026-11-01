//! Coalesced local light writes share native storage and normal mesh publication fences.
use super::*;
impl Terrain {
    pub fn take_light_changes(&mut self) -> BTreeSet<[i32; 3]> {
        std::mem::take(&mut self.light_changes)
    }
    /// Only changed native cells are written. Server light packets still replace these arrays
    /// through update_light; local work never disables or fabricates authoritative updates.
    pub fn apply_local_lights(
        &mut self,
        updates: impl IntoIterator<Item = ([i32; 3], [u8; 2])>,
    ) -> Result<usize, &'static str> {
        let mut columns = BTreeMap::<[i32; 2], Vec<([i32; 3], [u8; 2])>>::new();
        for (p, levels) in updates {
            if levels.iter().any(|n| *n > 15) {
                return Err("invalid Java local light level");
            }
            columns
                .entry([p[0].div_euclid(16), p[2].div_euclid(16)])
                .or_default()
                .push((p, levels));
        }
        let Some(d) = self.dimension.clone() else {
            return Ok(0);
        };
        let mut changed = 0;
        for (key, updates) in columns {
            let Some(column) = self.columns.get(&key) else {
                continue;
            };
            let before = column.light.clone();
            let mut next = before.clone();
            let mut count = 0;
            for (p, levels) in updates {
                let y = i64::from(p[1]) - i64::from(d.min_y);
                if y < -16 || y >= i64::from(d.height) + 16 || before.sample(&d, p) == Some(levels)
                {
                    continue;
                }
                let section = (y.div_euclid(16) + 1) as usize;
                let index = (p[1].rem_euclid(16) * 256
                    + p[2].rem_euclid(16) * 16
                    + p[0].rem_euclid(16)) as usize;
                for (channel, level) in levels.into_iter().enumerate() {
                    if before
                        .sample(&d, p)
                        .is_some_and(|old| old[channel] == level)
                    {
                        continue;
                    }
                    let layers = if channel == 0 {
                        &mut next.block
                    } else {
                        &mut next.sky
                    };
                    let Some(layer) = layers.get_mut(section) else {
                        continue;
                    };
                    if layer.as_ref().is_none_or(|l| l.is_empty()) {
                        // Materialize inherited skylight, preserving every other cell/row.
                        let mut bytes = vec![0u8; 2048];
                        let base = [
                            p[0].div_euclid(16) * 16,
                            d.min_y + (section as i32 - 1) * 16,
                            p[2].div_euclid(16) * 16,
                        ];
                        for i in 0..4096 {
                            let q = [
                                base[0] + (i % 16) as i32,
                                base[1] + (i / 256) as i32,
                                base[2] + ((i / 16) % 16) as i32,
                            ];
                            let n = before
                                .sample(&d, q)
                                .ok_or("invalid Java local light source")?[channel];
                            bytes[i / 2] |= n << ((i & 1) * 4);
                        }
                        *layer = Some(Arc::from(bytes));
                    }
                    let bytes = Arc::make_mut(layer.as_mut().unwrap());
                    let shift = (index & 1) * 4;
                    bytes[index / 2] = (bytes[index / 2] & !(15 << shift)) | (level << shift);
                }
                count += 1;
            }
            if count == 0 {
                continue;
            }
            let sections = before.changed_sections(&next, &d);
            let revision = self.next_revision();
            let column = Arc::make_mut(self.columns.get_mut(&key).unwrap());
            column.light = next;
            column.revision = revision;
            for y in sections {
                self.mark_light_section(key, y);
            }
            changed += count;
        }
        Ok(changed)
    }
}
