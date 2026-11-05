//! Orthographic GUI thumbnails of selected native quads; no world lighting or inventory prediction.
use super::{Image, Quad};
use java_item_model::math::{Mat4, quaternion_from_axis_angle};
const SIDE: usize = 64;
#[derive(Clone, Copy)]
struct Fragment {
    z: f32,
    rgba: [f32; 4],
}
pub(super) fn render(quads: &[Quad]) -> Image {
    let mut pixels: Vec<Vec<Fragment>> = (0..SIDE * SIDE).map(|_| Vec::new()).collect();
    for quad in quads {
        let a = sub(quad.positions[1], quad.positions[0]);
        let b = sub(quad.positions[2], quad.positions[0]);
        let n = cross(a, b);
        let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length <= f32::EPSILON || (quad.cull_back && n[2] <= 0.) {
            continue;
        }
        let normal = quad.normal.unwrap_or_else(|| n.map(|v| v / length));
        let light = lighting(normal, quad.block_light);
        for indices in [[0, 1, 2], [0, 2, 3]] {
            let mut p = indices.map(|i| {
                [
                    (quad.positions[i][0] + 0.5) * SIDE as f32,
                    (0.5 - quad.positions[i][1]) * SIDE as f32,
                ]
            });
            let mut uv = indices.map(|i| quad.uv[i]);
            let mut z = indices.map(|i| quad.positions[i][2]);
            let mut area = edge(p[0], p[1], p[2]);
            if area == 0. {
                continue;
            }
            if area < 0. {
                p.swap(1, 2);
                uv.swap(1, 2);
                z.swap(1, 2);
                area = -area;
            }
            let lo = std::array::from_fn::<_, 2, _>(|i| {
                p.iter()
                    .map(|v| v[i])
                    .fold(f32::INFINITY, f32::min)
                    .floor()
                    .clamp(0., SIDE as f32) as usize
            });
            let hi = std::array::from_fn::<_, 2, _>(|i| {
                p.iter()
                    .map(|v| v[i])
                    .fold(f32::NEG_INFINITY, f32::max)
                    .ceil()
                    .clamp(0., SIDE as f32) as usize
            });
            for y in lo[1]..hi[1] {
                for x in lo[0]..hi[0] {
                    let point = [x as f32 + 0.5, y as f32 + 0.5];
                    let edges = [
                        edge(p[1], p[2], point),
                        edge(p[2], p[0], point),
                        edge(p[0], p[1], point),
                    ];
                    if ![(p[1], p[2]), (p[2], p[0]), (p[0], p[1])]
                        .iter()
                        .zip(edges)
                        .all(|(&(a, b), e)| e > 0. || (e == 0. && top_left(a, b)))
                    {
                        continue;
                    }
                    let w = edges.map(|e| e / area);
                    let tex = std::array::from_fn::<_, 2, _>(|k| {
                        (0..3).map(|i| w[i] * uv[i][k]).sum::<f32>()
                    });
                    let tx = (tex[0] * quad.image.width as f32)
                        .floor()
                        .clamp(0., (quad.image.width - 1) as f32)
                        as usize;
                    let ty = (tex[1] * quad.image.height as f32)
                        .floor()
                        .clamp(0., (quad.image.height - 1) as f32)
                        as usize;
                    let src = &quad.image.rgba[(ty * quad.image.width as usize + tx) * 4..][..4];
                    let rgba = std::array::from_fn(|i| {
                        src[i] as f32 / 255. * quad.color[i] * if i == 3 { 1. } else { light }
                    });
                    if rgba[3] > 0. {
                        pixels[y * SIDE + x].push(Fragment {
                            z: (0..3).map(|i| w[i] * z[i]).sum(),
                            rgba,
                        });
                    }
                }
            }
        }
    }
    let mut rgba = Vec::with_capacity(SIDE * SIDE * 4);
    for fragments in &mut pixels {
        fragments.sort_by(|a, b| a.z.total_cmp(&b.z));
        let mut c = [0f32; 4];
        for f in fragments {
            let a = f.rgba[3];
            for (i, v) in c.iter_mut().enumerate().take(3) {
                *v = f.rgba[i] * a + *v * (1. - a)
            }
            c[3] = a + c[3] * (1. - a);
        }
        if c[3] > 0. {
            for i in 0..3 {
                c[i] /= c[3];
            }
        }
        rgba.extend(c.map(|v| (v.clamp(0., 1.) * 255.).round() as u8));
    }
    Image {
        width: SIDE as u32,
        height: SIDE as u32,
        rgba,
    }
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn top_left(a: [f32; 2], b: [f32; 2]) -> bool {
    b[1] < a[1] || (b[1] == a[1] && b[0] > a[0])
}
fn lighting(normal: [f32; 3], block: bool) -> f32 {
    // Verified 26.3 Lighting ITEMS_FLAT / ITEMS_3D transforms and light.glsl's 0.6/0.4 mix.
    let y = |v: f32| quaternion_from_axis_angle(v.to_radians(), [0., 1., 0.]);
    let x = |v: f32| quaternion_from_axis_angle(v.to_radians(), [1., 0., 0.]);
    let matrix = if block {
        Mat4::IDENTITY
            .scale([1., -1., 1.])
            .rotate(y(62.))
            .rotate(x(185.5))
            .rotate(y(-22.5))
            .rotate(x(135.))
    } else {
        Mat4::IDENTITY.rotate(y(-22.5)).rotate(x(135.))
    };
    let sum: [[f32; 3]; 2] = [[0.2, 1., -0.7], [-0.2, 1., 0.7]];
    let light = sum
        .iter()
        .map(|v| {
            let len = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            let v = v.map(|n| n / len);
            let m = &matrix.0;
            let dir = std::array::from_fn::<_, 3, _>(|i| {
                m[0][i] * v[0] + m[1][i] * v[1] + m[2][i] * v[2]
            });
            (0..3).map(|i| dir[i] * normal[i]).sum::<f32>().max(0.)
        })
        .sum::<f32>();
    (light * 0.6 + 0.4).min(1.)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn panel(z: f32, alpha: u8) -> Quad {
        Quad {
            positions: [
                [-0.5, -0.5, z],
                [0.5, -0.5, z],
                [0.5, 0.5, z],
                [-0.5, 0.5, z],
            ],
            uv: [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
            image: Arc::new(Image {
                width: 2,
                height: 2,
                rgba: vec![
                    255, 0, 0, alpha, 0, 255, 0, alpha, 0, 0, 255, alpha, 255, 255, 255, alpha,
                ],
            }),
            color: [1.; 4],
            block_light: false,
            cull_back: true,
            normal: None,
        }
    }
    #[test]
    fn quad_diagonal_has_one_alpha_layer_and_uvs_keep_orientation() {
        let image = render(&[panel(0., 128)]);
        assert!(image.rgba.chunks_exact(4).all(|p| p[3] == 128));
        let p = |x: usize, y: usize| &image.rgba[(y * SIDE + x) * 4..][..4];
        assert!(p(8, 8)[0] > 0 && p(8, 8)[1] == 0 && p(8, 8)[2] == 0);
        assert!(p(56, 8)[1] > 0 && p(56, 8)[0] == 0);
        assert!(p(8, 56)[2] > 0 && p(8, 56)[0] == 0);
    }
    #[test]
    fn depth_and_alpha_are_ordered_independently_of_submission() {
        let far = panel(-0.1, 128);
        let mut near = panel(0.1, 255);
        near.image = Arc::new(Image {
            width: 1,
            height: 1,
            rgba: vec![30, 60, 90, 255],
        });
        assert_eq!(
            render(&[far.clone(), near.clone()]).rgba,
            render(&[near.clone(), far]).rgba
        );
        assert!(render(&[near]).rgba.chunks_exact(4).all(|p| p[3] == 255));
    }
}
