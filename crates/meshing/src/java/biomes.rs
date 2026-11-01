//! Java 26.3 BiomeManager selection and default 5x5 block-colour averaging.
//! Behaviour reference: verified official server BiomeManager/LinearCongruentialGenerator
//! bytecode and native resolver oracle. The received seed is already obfuscated; never hash again.
use super::{Error, Snapshot};
use crate::PackedBiomeRecord;
use std::collections::{BTreeMap, BTreeSet};

pub const MAGIC: u32 = 0x4a42_4931;
pub const RADIUS: i32 = 2;
pub const SAMPLES: usize = ((RADIUS * 2 + 1) * (RADIUS * 2 + 1)) as usize;
pub const MIN_Y: i32 = -1;
const _: () = assert!(SAMPLES <= crate::biome_lattice::BLEND_SAMPLE_COUNT);
pub const CELLS: usize = 16 * 17 * 16;
pub const MAX_WORDS: usize = 4 + CELLS * (1 + SAMPLES) + CELLS * (3 + 2 * SAMPLES);

/// Returns the authoritative quart cell selected by the seeded eight-corner Voronoi zoom.
pub fn quart(seed: i64, position: [i32; 3]) -> [i32; 3] {
    let shifted = position.map(|n| n.wrapping_sub(2));
    let base = shifted.map(|n| n >> 2);
    let fraction = shifted.map(|n| f64::from(n & 3) / 4.0);
    let mut best = f64::INFINITY;
    let mut chosen = base;
    for corner in 0..8 {
        let offset = [corner >> 2, (corner >> 1) & 1, corner & 1];
        let cell = std::array::from_fn(|i| base[i].wrapping_add(offset[i]));
        let mut mixed = seed;
        for axis in [0, 1, 2, 0, 1, 2] {
            mixed = next(mixed, i64::from(cell[axis]));
        }
        let mut delta = [0.; 3];
        for axis in 0..3 {
            delta[axis] = fraction[axis] - f64::from(offset[axis]) + fiddle(mixed);
            mixed = next(mixed, seed);
        }
        // Match the native Z,Y,X addition order and first-corner tie resolution.
        let distance = delta[2] * delta[2] + delta[1] * delta[1] + delta[0] * delta[0];
        if distance < best {
            best = distance;
            chosen = cell;
        }
    }
    chosen
}
fn next(seed: i64, salt: i64) -> i64 {
    seed.wrapping_mul(
        seed.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407),
    )
    .wrapping_add(salt)
}
fn fiddle(seed: i64) -> f64 {
    (((seed >> 24).rem_euclid(1024) as f64) / 1024. - 0.5) * 0.9
}
fn sample(snapshot: &Snapshot, seed: i64, position: [i32; 3]) -> Result<u32, Error> {
    let [x, y, z] = quart(seed, position);
    let column = snapshot
        .columns
        .get(&[x.div_euclid(4), z.div_euclid(4)])
        .ok_or(Error::MissingBiome(position))?;
    let y = y.clamp(
        snapshot.dimension.min_y.div_euclid(4),
        (snapshot.dimension.min_y + snapshot.dimension.height - 1).div_euclid(4),
    );
    let local_y = y - snapshot.dimension.min_y.div_euclid(4);
    column
        .sections
        .get(local_y as usize / 4)
        .and_then(|s| {
            s.biomes.get(
                ((local_y as usize % 4) << 4)
                    | ((z.rem_euclid(4) as usize) << 2)
                    | x.rem_euclid(4) as usize,
            )
        })
        .map(|id| id.0)
        .ok_or(Error::MissingBiome(position))
}
pub fn record(snapshot: &Snapshot, section_y: i32) -> Result<PackedBiomeRecord, Error> {
    let seed = snapshot
        .biome_seed
        .ok_or_else(|| Error::Invalid("Java spawn biome seed not received".into()))?;
    let origin = |n: i32| {
        n.checked_mul(16)
            .ok_or_else(|| Error::Invalid("Java biome origin overflow".into()))
    };
    let [x0, y0, z0] = [
        origin(snapshot.key.chunk[0])?,
        origin(section_y)?,
        origin(snapshot.key.chunk[1])?,
    ];
    let side = 16 + 2 * RADIUS;
    let mut grid = Vec::with_capacity((side * side * 17) as usize);
    for y in MIN_Y..16 {
        for z in -RADIUS..16 + RADIUS {
            for x in -RADIUS..16 + RADIUS {
                let position = [x0.checked_add(x), y0.checked_add(y), z0.checked_add(z)];
                let [Some(x), Some(y), Some(z)] = position else {
                    return Err(Error::Invalid("Java biome sample overflow".into()));
                };
                grid.push(sample(snapshot, seed, [x, y, z])?);
            }
        }
    }
    let at = |x: i32, y: i32, z: i32| {
        grid[(((y - MIN_Y) * side + z + RADIUS) * side + x + RADIUS) as usize]
    };
    let mut used = BTreeSet::new();
    let mut cells = Vec::with_capacity(CELLS);
    for y in MIN_Y..16 {
        for z in 0..16 {
            for x in 0..16 {
                let direct = at(x, y, z);
                let mut counts = BTreeMap::<u32, u32>::new();
                for dz in -RADIUS..=RADIUS {
                    for dx in -RADIUS..=RADIUS {
                        let id = at(x + dx, y, z + dz);
                        *counts.entry(id).or_default() += 1;
                        used.insert(id);
                    }
                }
                cells.push((direct, counts));
            }
        }
    }
    if used.len() == 1 {
        return PackedBiomeRecord::uniform(*used.first().unwrap())
            .ok_or_else(|| Error::Invalid("Java tint index exceeds descriptor".into()));
    }
    let mut words = vec![MAGIC, u32::MAX, 0, used.len() as u32];
    words.extend(used);
    let offsets = words.len();
    words.resize(offsets + CELLS, 0);
    for (i, (direct, counts)) in cells.into_iter().enumerate() {
        words[offsets + i] = words.len() as u32;
        words.extend([direct, counts.len() as u32]);
        for (id, count) in counts {
            words.extend([id, count]);
        }
    }
    words[2] = words.len() as u32;
    debug_assert!(words.len() <= MAX_WORDS);
    Ok(PackedBiomeRecord {
        words: words.into(),
    })
}
impl PackedBiomeRecord {
    pub fn java_tint_indices(&self) -> Option<&[u32]> {
        (self.words[0] == MAGIC).then(|| &self.words[4..4 + self.words[3] as usize])
    }
    pub(crate) fn java_cell(&self, [x, y, z]: [i32; 3]) -> Option<&[u32]> {
        if self.words[0] != MAGIC
            || !(0..16).contains(&x)
            || !(MIN_Y..16).contains(&y)
            || !(0..16).contains(&z)
        {
            return None;
        }
        let i = (((y - MIN_Y) * 16 + z) * 16 + x) as usize;
        let start = self.words[4 + self.words[3] as usize + i] as usize;
        Some(&self.words[start..start + 2 + 2 * self.words[start + 1] as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use world::java::{BiomeId, BlockStateId, Column, Dimension, Light, Palette, Section, Terrain};

    #[test]
    fn selection_matches_native_26_3_oracle() {
        for row in include_str!("biome-zoom-26.3.csv").lines() {
            let n = row
                .split(',')
                .map(|s| s.parse::<i64>().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                quart(n[0], [n[1] as i32, n[2] as i32, n[3] as i32]),
                [n[4] as i32, n[5] as i32, n[6] as i32],
                "{row}"
            );
        }
    }
    fn terrain() -> Terrain {
        let mut t = Terrain::default();
        t.begin_seeded(
            Dimension {
                session: 1,
                epoch: 1,
                name: "fixture:biomes".into(),
                min_y: -16,
                height: 32,
                has_skylight: true,
            },
            1234,
        );
        for x in -2..=1 {
            for z in -2..=1 {
                let sections = (0..2)
                    .map(|sy| {
                        let mut biomes = Palette::uniform(BiomeId(3), 64);
                        for y in 0..4 {
                            for lz in 0..4 {
                                for lx in 0..4 {
                                    // Deliberately different vertical and horizontal authoritative quart cells.
                                    let id = if x * 4 + lx < -1 {
                                        7
                                    } else if sy == 0 && y < 2 {
                                        11
                                    } else {
                                        3
                                    };
                                    biomes.set(((y << 4) | (lz << 2) | lx) as usize, BiomeId(id));
                                }
                            }
                        }
                        Arc::new(Section {
                            blocks: Palette::uniform(BlockStateId(0), 4096),
                            biomes,
                        })
                    })
                    .collect();
                t.replace(
                    [x, z],
                    Column::new(
                        sections,
                        Light {
                            sky: vec![Some(Arc::from([])); 4],
                            block: vec![Some(Arc::from([])); 4],
                        },
                        vec![],
                        vec![],
                    ),
                );
            }
        }
        t
    }
    #[test]
    fn exact_blend_weights_vertical_variation_and_upper_half_sample() {
        let t = terrain();
        let s = Snapshot::capture(&t, [-1, -1]).unwrap();
        let r = record(&s, -1).unwrap();
        assert!(r.words().len() <= MAX_WORDS);
        assert!(r.java_tint_indices().unwrap().contains(&11));
        for y in MIN_Y..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let cell = r.java_cell([x, y, z]).unwrap();
                    assert!((1..=SAMPLES as u32).contains(&cell[1]));
                    assert_eq!(
                        cell[2..].chunks_exact(2).map(|p| p[1]).sum::<u32>(),
                        SAMPLES as u32
                    );
                    assert!(
                        cell[2..]
                            .chunks_exact(2)
                            .all(|p| [3, 7, 11].contains(&p[0]))
                    );
                    assert_eq!(
                        r.tint_index_at([x, y, z]),
                        Some(sample(&s, 1234, [-16 + x, -16 + y, -16 + z]).unwrap())
                    );
                }
            }
        }
        assert_eq!(
            r.blend_samples_below([15, 0, 15]),
            r.blend_samples([15, -1, 15])
        );
    }
    #[test]
    fn blend_histograms_match_the_native_resolver() {
        let t = terrain();
        let s = Snapshot::capture(&t, [-1, -1]).unwrap();
        let r = record(&s, -1).unwrap();
        for row in include_str!("biome-blend-26.3.csv").lines() {
            let mut fields = row.split(',');
            let position = std::array::from_fn::<_, 3, _>(|_| {
                fields.next().unwrap().parse::<i32>().unwrap() + 16
            });
            let expected = fields
                .flat_map(|field| field.split(':').map(|n| n.parse::<u32>().unwrap()))
                .collect::<Vec<_>>();
            assert_eq!(
                &r.java_cell(position).unwrap()[2..],
                expected.as_slice(),
                "{row}"
            );
        }
    }
    #[test]
    fn missing_neighbour_and_seed_are_explicit_and_recover_on_capture() {
        let mut t = terrain();
        let s = Snapshot::capture(&t, [-1, -1]).unwrap();
        let mut missing = Snapshot::capture(&t, [-1, -1]).unwrap();
        missing.columns.remove(&[-2, -1]);
        assert!(matches!(record(&missing, -1), Err(Error::MissingBiome(_))));
        assert!(record(&s, -1).is_ok());
        t.take_dirty();
        let neighbour = (**t.column([-2, -1]).unwrap()).clone();
        t.unload([-2, -1]);
        assert!(!t.is_current(&s.key));
        assert!(matches!(
            record(&Snapshot::capture(&t, [-1, -1]).unwrap(), -1),
            Err(Error::MissingBiome(_))
        ));
        assert!(t.replace([-2, -1], neighbour));
        let restored = Snapshot::capture(&t, [-1, -1]).unwrap();
        assert_eq!(record(&s, -1).unwrap(), record(&restored, -1).unwrap());
        assert!(t.take_dirty().contains(&[-1, -1]));
        let old = restored.key.clone();
        t.update_biomes([-1, -1], vec![Palette::uniform(BiomeId(9), 64); 2]);
        assert!(!t.is_current(&old));
        let next = Snapshot::capture(&t, [-1, -1]).unwrap();
        assert!(
            record(&next, -1)
                .unwrap()
                .java_tint_indices()
                .unwrap()
                .contains(&9)
        );
        t.begin(t.dimension().unwrap().clone());
        assert!(t.biome_seed().is_none());
    }
    #[test]
    fn partitioning_uses_absolute_coordinates_and_received_seed() {
        let t = terrain();
        let a = Snapshot::capture(&t, [-1, -1]).unwrap();
        let b = Snapshot::capture(&t, [0, -1]).unwrap();
        for x in -2..=1 {
            for y in [-17, -16, -1, 0, 15, 16] {
                assert_eq!(
                    sample(&a, 1234, [x, y, -8]).unwrap(),
                    sample(&b, 1234, [x, y, -8]).unwrap()
                );
            }
        }
        let mut no_seed = a;
        no_seed.biome_seed = None;
        assert!(matches!(record(&no_seed, -1), Err(Error::Invalid(_))));
    }
}
