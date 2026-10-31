//! Native Java section meshing from domain snapshots, independent of Bedrock storages/IDs.
use crate::{ChunkMesh, FaceConnectivity, PackedModelDrawRef, PackedModelRef, PackedQuadLighting};
use assets::registry::java::{
    geometry,
    graphics::{Graphics, face_bit},
    models::Face,
    source::Error as AssetError,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use world::java::{Column, Dimension, PublicationKey};

const HALO: i32 = 1;
mod fluids;
/// Sections whose cell/light halo intersects an authoritative changed region.
/// Coalesced regions remain section-local; this never walks an unbounded world-volume box.
pub fn affected_sections(region: world::java::ChangedRegion) -> impl Iterator<Item = [i32; 3]> {
    let min = region.min.map(|v| v.saturating_sub(HALO).div_euclid(16));
    let max = region.max.map(|v| v.saturating_add(HALO).div_euclid(16));
    (min[0]..=max[0]).flat_map(move |x| {
        (min[1]..=max[1]).flat_map(move |y| (min[2]..=max[2]).map(move |z| [x, y, z]))
    })
}

/// Immutable halo captured with the publication key. The caller rejects outdated results.
pub struct Snapshot {
    pub dimension: Dimension,
    pub key: PublicationKey,
    pub columns: BTreeMap<[i32; 2], Arc<Column>>,
}
impl Snapshot {
    pub fn capture(terrain: &world::java::Terrain, chunk: [i32; 2]) -> Option<Self> {
        let dimension = terrain.dimension()?.clone();
        let key = terrain.publication_key(chunk)?;
        let columns = terrain
            .columns()
            .filter(|(c, _)| {
                (i64::from(c[0]) - i64::from(chunk[0])).abs() <= 1
                    && (i64::from(c[1]) - i64::from(chunk[1])).abs() <= 1
            })
            .map(|(k, v)| (k, Arc::clone(v)))
            .collect();
        Some(Self {
            dimension,
            key,
            columns,
        })
    }
    pub fn block(&self, [x, y, z]: [i32; 3]) -> Option<u32> {
        let local = y
            .checked_sub(self.dimension.min_y)
            .filter(|y| *y >= 0 && *y < self.dimension.height)?;
        let column = self.columns.get(&[x.div_euclid(16), z.div_euclid(16)])?;
        column.sections[local as usize / 16]
            .blocks
            .get(
                ((local as usize % 16) << 8)
                    | ((z.rem_euclid(16) as usize) << 4)
                    | x.rem_euclid(16) as usize,
            )
            .map(|id| id.0)
    }
    /// Samples native light storage without turning missing columns into fullbright terrain.
    pub fn light(&self, [x, y, z]: [i32; 3]) -> Option<[u8; 2]> {
        self.columns
            .get(&[x.div_euclid(16), z.div_euclid(16)])?
            .light
            .sample(&self.dimension, [x, y, z])
    }
}
/// Builds the selected absolute section Y. Flat server lighting is supported; smooth Java AO,
/// and partial-face occlusion remain separate work. Native fluids use their own state semantics.
pub fn mesh(snapshot: &Snapshot, section_y: i32, graphics: &Graphics) -> Result<ChunkMesh, Error> {
    let [cx, cz] = snapshot.key.chunk;
    let origin = |v: i32| {
        v.checked_mul(16)
            .filter(|n| n.checked_sub(HALO).is_some() && n.checked_add(15 + HALO).is_some())
            .ok_or_else(|| invalid("Java section coordinate overflow"))
    };
    let [x0, y0, z0] = [origin(cx)?, origin(section_y)?, origin(cz)?];
    let local_y = y0
        .checked_sub(snapshot.dimension.min_y)
        .ok_or_else(|| invalid("section Y"))?;
    if local_y < 0 || local_y >= snapshot.dimension.height {
        return Err(invalid("section outside Java dimension"));
    }
    let mut refs = Vec::new();
    let mut lighting = Vec::new();
    let mut opaque = Vec::new();
    let mut transparent = Vec::new();
    // Resolve each block once, including the halo; positional variants remain deterministic.
    let mut cells = BTreeMap::new();
    let mut missing = BTreeSet::new();
    for y in -HALO..=15 + HALO {
        for z in -HALO..=15 + HALO {
            for x in -HALO..=15 + HALO {
                let pos = [x0 + x, y0 + y, z0 + z];
                if let Some(id) = snapshot.block(pos) {
                    if graphics.contains(id) {
                        cells.insert(pos, graphics.select(id, pos)?);
                    } else {
                        missing.insert(id);
                    }
                }
            }
        }
    }
    if !missing.is_empty() {
        return Err(Error::MissingGraphics(missing));
    }
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                let position = [x0 + x, y0 + y, z0 + z];
                let selected = cells
                    .get(&position)
                    .ok_or_else(|| invalid("missing resident Java block"))?;
                for &id in selected {
                    let template = &graphics.templates[&id];
                    let gpu = &graphics.runtime.model_templates()[id as usize];
                    let base = lighting.len() as u32;
                    let mut mask = 0u32;
                    let reference = refs.len() as u32;
                    for (q, quad) in template.quads.iter().enumerate() {
                        let n = geometry::normal(quad.cull_face.unwrap_or(quad.facing))
                            .to_array()
                            .map(|v| v as i32);
                        let neighbour = std::array::from_fn(|i| position[i] + n[i]);
                        let hidden = quad.cull_face.is_some_and(|face| {
                            cells.get(&neighbour).is_some_and(|parts| {
                                parts.iter().any(|id| {
                                    graphics.templates[id].occludes & face_bit(opposite(face)) != 0
                                })
                            })
                        });
                        if hidden {
                            lighting.push(PackedQuadLighting::default());
                            continue;
                        }
                        // Boundary planes sample outside; inset geometry samples its own cell.
                        let boundary = quad.cull_face.is_some()
                            && quad.positions.iter().all(|p| {
                                let dot = p.iter().zip(n).map(|(p, n)| *p * n as f32).sum::<f32>();
                                (dot - if n.iter().any(|v| *v > 0) { 1. } else { 0. }).abs()
                                    < 0.00001
                            });
                        let sample = if boundary { neighbour } else { position };
                        let light = snapshot.light(sample).ok_or(Error::MissingLight(sample))?;
                        let packed = u16::from(light[0].max(quad.light_emission))
                            | (u16::from(light[1]) << 4);
                        lighting.push(PackedQuadLighting::new([packed; 4]));
                        mask |= 1 << q;
                        let material = graphics.runtime.material(
                            graphics.runtime.model_quads()[gpu.quad_start as usize + q].material,
                        );
                        let draw = PackedModelDrawRef::new(reference, q as u32);
                        if material.flags & assets::MATERIAL_FLAG_ALPHA_BLEND != 0 {
                            transparent.push(draw);
                        } else {
                            opaque.push(draw);
                        }
                    }
                    if mask != 0 {
                        refs.push(PackedModelRef::new(
                            x as u32 | ((y as u32) << 4) | ((z as u32) << 8),
                            id,
                            base,
                            mask,
                        ));
                    } else {
                        lighting.truncate(base as usize);
                    }
                }
            }
        }
    }
    let mut liquids = Vec::new();
    let mut liquid_lighting = Vec::new();
    fluids::emit(
        snapshot,
        [x0, y0, z0],
        graphics,
        &mut liquids,
        &mut liquid_lighting,
    )?;
    // The upload ABI requires one transparent prefix followed by a depth-writing suffix.
    // Lighting is addressed by the index carried in each quad, not by stream order.
    liquids.sort_by_key(|quad| quad.is_depth_writing());
    // Every shared stream is assembled here; no Bedrock network or visual identity is involved.
    let result = ChunkMesh::try_from_streams_with_cube_lighting(
        Vec::new(),
        Vec::new(),
        refs,
        lighting,
        opaque,
        liquids,
        liquid_lighting,
        FaceConnectivity::all(),
    )
    .expect("empty cube streams agree");
    Ok(result.with_transparent_model_draw_refs(transparent))
}
fn opposite(face: Face) -> Face {
    match face {
        Face::Down => Face::Up,
        Face::Up => Face::Down,
        Face::West => Face::East,
        Face::East => Face::West,
        Face::North => Face::South,
        Face::South => Face::North,
    }
}
#[derive(Debug)]
pub enum Error {
    Assets(AssetError),
    MissingLight([i32; 3]),
    MissingGraphics(BTreeSet<u32>),
    Invalid(String),
}
impl From<AssetError> for Error {
    fn from(value: AssetError) -> Self {
        Self::Assets(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Assets(e) => e.fmt(f),
            Self::MissingLight(p) => write!(f, "Java light not yet known at {p:?}"),
            Self::MissingGraphics(ids) => write!(f, "Java states awaiting graphics: {ids:?}"),
            Self::Invalid(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for Error {}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[cfg(test)]
mod fluid_tests;
#[cfg(test)]
mod tests;
