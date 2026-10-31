//! Java FluidRenderer geometry over immutable native snapshots; no Bedrock state resolution.
use super::{Error, Snapshot};
use crate::{Face, PackedLiquidQuad, PackedQuadLighting};
use assets::registry::java::{
    fluids::{Kind, State, Table},
    graphics::Graphics,
};

const AIR: State = State {
    id: 0,
    kind: Kind::Empty,
    height: 0.,
    falling: false,
    solid: false,
    solid_render: false,
    flow_blocking: false,
    overlay: false,
    invisible: true,
    emission: 0,
    occlusion: [0.; 6],
};
const DELTA: [[i32; 3]; 6] = [
    [-1, 0, 0],
    [1, 0, 0],
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
];
fn offset(p: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| p[i] + d[i])
}

struct Cells<'a> {
    snapshot: &'a Snapshot,
    table: &'a Table,
}
impl Cells<'_> {
    fn get(&self, p: [i32; 3]) -> &State {
        // Missing halo cells temporarily expose faces; arrival/unload invalidates the halo.
        self.snapshot
            .block(p)
            .and_then(|id| self.table.state(id))
            .unwrap_or(&AIR)
    }
    fn height(&self, kind: Kind, p: [i32; 3]) -> f32 {
        let s = self.get(p);
        if s.kind == kind {
            if self.get(offset(p, [0, 1, 0])).kind == kind {
                1.
            } else {
                s.height
            }
        } else if s.solid {
            -1.
        } else {
            0.
        }
    }
    fn corner(&self, kind: Kind, p: [i32; 3], center: f32, x: i32, z: i32) -> f32 {
        let a = self.height(kind, offset(p, [x, 0, 0]));
        let b = self.height(kind, offset(p, [0, 0, z]));
        if a >= 1. || b >= 1. {
            return 1.;
        }
        let diagonal = if a > 0. || b > 0. {
            self.height(kind, offset(p, [x, 0, z]))
        } else {
            -1.
        };
        if diagonal >= 1. {
            return 1.;
        }
        let mut weight = 0.;
        let mut sum = 0.;
        for h in [diagonal, center, b, a] {
            if h >= 0. {
                let w = if h >= 0.8 { 10. } else { 1. };
                sum += h * w;
                weight += w;
            }
        }
        sum / weight
    }
    fn flow(&self, state: &State, p: [i32; 3]) -> [i8; 2] {
        let mut flow = [0f32; 2];
        for face in [4, 5, 0, 1] {
            let n = offset(p, DELTA[face]);
            let other = self.get(n);
            if other.kind != Kind::Empty && other.kind != state.kind {
                continue;
            }
            let mut slope = 0.;
            if other.height == 0. {
                if !other.flow_blocking {
                    let below = self.get(offset(n, [0, -1, 0]));
                    if below.kind == state.kind && below.height > 0. {
                        slope = state.height - (below.height - 8. / 9.);
                    }
                }
            } else {
                slope = state.height - other.height;
            }
            flow[0] += DELTA[face][0] as f32 * slope;
            flow[1] += DELTA[face][2] as f32 * slope;
        }
        // Falling adds vertical flow but does not change the horizontal UV angle.
        let scale = flow[0].abs().max(flow[1].abs());
        if scale < 1e-6 {
            [0, 0]
        } else {
            flow.map(|v| (v / scale * 127.).round() as i8)
        }
    }
    fn light(&self, p: [i32; 3]) -> PackedQuadLighting {
        let above = offset(p, [0, 1, 0]);
        let a = self.snapshot.light(p).unwrap_or([0; 2]);
        let b = self.snapshot.light(above).unwrap_or([0; 2]);
        let block = a[0]
            .max(b[0])
            .max(self.get(p).emission)
            .max(self.get(above).emission);
        let packed = u16::from(block) | u16::from(a[1].max(b[1])) << 4;
        PackedQuadLighting::new([packed; 4])
    }
}

/// Emits native liquid streams alongside the host's ordinary model. Output is bounded to six
/// quads per cell; winding, texture slots and lighting use the renderer's existing packed ABI.
pub(super) fn emit(
    snapshot: &Snapshot,
    origin: [i32; 3],
    graphics: &Graphics,
    quads: &mut Vec<PackedLiquidQuad>,
    lighting: &mut Vec<PackedQuadLighting>,
) -> Result<(), Error> {
    let Some(table) = &graphics.fluid_facts else {
        return Ok(());
    };
    let cells = Cells { snapshot, table };
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                let local = [x, y, z];
                let p = offset(origin, local);
                let state = cells.get(p);
                if state.kind == Kind::Empty {
                    continue;
                }
                let material = graphics
                    .fluid_materials(state.kind)
                    .ok_or_else(|| super::invalid("Java fluid materials not prepared"))?;
                let h = cells.height(state.kind, p);
                let heights = if h >= 1. {
                    [1.; 4]
                } else {
                    [(-1, -1), (1, -1), (1, 1), (-1, 1)]
                        .map(|(dx, dz)| cells.corner(state.kind, p, h, dx, dz))
                };
                let top_neighbor = cells.get(offset(p, [0, 1, 0]));
                let top = top_neighbor.kind != state.kind
                    && !top_neighbor.covers(3, heights.into_iter().fold(1., f32::min));
                let bottom_neighbor = cells.get(offset(p, [0, -1, 0]));
                let bottom = bottom_neighbor.kind != state.kind
                    && !state.covers(3, 1.)
                    && !bottom_neighbor.covers(2, 8. / 9.);
                let flow = cells.flow(state, p);
                for face in Face::ALL {
                    let i = face as usize;
                    let neighbor = cells.get(offset(p, DELTA[i]));
                    let (ys, texture, two_sided, light_position) = match face {
                        Face::PositiveY if top => {
                            let reverse = (-1..=1).any(|dx| {
                                (-1..=1).any(|dz| {
                                    let above = cells.get(offset(p, [dx, 1, dz]));
                                    above.kind != state.kind && !above.solid_render
                                })
                            });
                            (
                                heights,
                                if flow == [0; 2] {
                                    material.still
                                } else {
                                    material.flowing
                                },
                                reverse,
                                p,
                            )
                        }
                        Face::NegativeY if bottom => {
                            ([0.; 4], material.still, false, offset(p, [0, -1, 0]))
                        }
                        Face::NegativeX | Face::PositiveX | Face::NegativeZ | Face::PositiveZ => {
                            if neighbor.kind == state.kind || state.covers(i ^ 1, 1.) {
                                continue;
                            }
                            let [a, b] = match face {
                                Face::NegativeX => [heights[0], heights[3]],
                                Face::PositiveX => [heights[2], heights[1]],
                                Face::NegativeZ => [heights[1], heights[0]],
                                _ => [heights[3], heights[2]],
                            };
                            if neighbor.covers(i, a.max(b)) {
                                continue;
                            }
                            let overlay = material.overlay.filter(|_| neighbor.overlay);
                            (
                                [0., a, b, 0.],
                                overlay.unwrap_or(material.flowing),
                                overlay.is_none(),
                                p,
                            )
                        }
                        _ => continue,
                    };
                    let index = lighting.len() as u32;
                    lighting.push(cells.light(light_position));
                    let quad = PackedLiquidQuad::try_pack(
                        local.map(|v| v as u8),
                        face,
                        ys.map(|h| (h * 255.).round().clamp(0., 255.) as u8),
                        texture,
                        index,
                        flow,
                        state.falling,
                    )
                    .ok_or_else(|| super::invalid("Java liquid packing overflow"))?
                    .with_java_fluid(true)
                    .with_bottom_inset(bottom)
                    .with_depth_write(state.kind == Kind::Lava)
                    .with_top_height_inset(top)
                    .with_two_sided(two_sided);
                    quads.push(quad);
                }
            }
        }
    }
    Ok(())
}
