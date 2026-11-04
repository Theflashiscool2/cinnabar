//! Native living-entity ray selection: current simulation boxes, never render mesh bounds.
use assets::registry::java::physics::Bounds;

/// Returns the first box entry before both the block occluder and entity interaction range.
/// ProjectileUtil handles inside origins separately; LocalPlayer filters range strictly.
pub fn entry(
    from: [f64; 3],
    to: [f64; 3],
    bounds: Bounds,
    range: f64,
    block_distance: f64,
) -> Option<f64> {
    if from
        .into_iter()
        .chain(to)
        .chain(bounds.min)
        .chain(bounds.max)
        .any(|v| !v.is_finite())
        || !range.is_finite()
        || range < 0.
        || (0..3).any(|i| bounds.min[i] >= bounds.max[i])
    {
        return None;
    }
    let inside = (0..3).all(|i| from[i] >= bounds.min[i] && from[i] < bounds.max[i]);
    let mut near: f64 = 0.;
    let mut far: f64 = 1.;
    for i in 0..3 {
        let delta = to[i] - from[i];
        if delta.abs() < 1e-7 {
            if from[i] < bounds.min[i] || from[i] > bounds.max[i] {
                return None;
            }
        } else {
            let a = (bounds.min[i] - from[i]) / delta;
            let b = (bounds.max[i] - from[i]) / delta;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    // AABB.clip admits entry planes strictly inside the segment. The separate
    // contains branch includes the minimum faces but excludes maximum faces.
    if !inside && (near <= 0. || near >= 1.) {
        return None;
    }
    let t = if inside { 0. } else { near };
    let distance = t * from
        .into_iter()
        .zip(to)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt();
    (distance < range && distance < block_distance).then_some(distance)
}

#[cfg(test)]
mod tests {
    use super::*;
    const BOX: Bounds = Bounds {
        min: [-0.3, 0., 2.7],
        max: [0.3, 1.8, 3.3],
    };
    #[test]
    fn pinned_native_clip_reference_corpus() {
        let reference: serde_json::Value =
            serde_json::from_str(include_str!("entity_selection/reference.json")).unwrap();
        for (index, case) in reference["cases"].as_array().unwrap().iter().enumerate() {
            let vector = |key: &str| std::array::from_fn(|i| case[key][i].as_f64().unwrap());
            let actual = entry(vector("from"), vector("to"), BOX, 100., 100.);
            let expected = case["distance"].as_f64();
            match (actual, expected) {
                (Some(a), Some(b)) => assert!((a - b).abs() < 1e-12, "case {index}: {a} != {b}"),
                (a, b) => assert_eq!(a, b, "case {index}"),
            }
        }
    }
    #[test]
    fn nearest_face_strict_reach_and_block_occlusion() {
        let from = [0., 1.62, 0.];
        let to = [0., 1.62, 5.];
        assert_eq!(entry(from, to, BOX, 3., 5.), Some(2.7));
        assert_eq!(entry(from, to, BOX, 2.7, 5.), None);
        assert_eq!(entry(from, to, BOX, 3., 2.7), None);
        assert_eq!(entry(from, to, BOX, 3., 2.), None);
        assert_eq!(entry([0.4, 1., 0.], [0.4, 1., 5.], BOX, 3., 5.), None);
    }
    #[test]
    fn crouch_inside_and_negative_translation() {
        let from = [0., 1.62, 0.];
        let to = [0., 1.62, 5.];
        let crouch = Bounds {
            max: [0.3, 1.5, 3.3],
            ..BOX
        };
        assert_eq!(entry(from, to, crouch, 3., 5.), None);
        assert_eq!(entry([0., 1., 3.], to, BOX, 3., 5.), Some(0.));
        for offset in [[-32., -64., -16.], [16., 10., 32.]] {
            let shift = |p: [f64; 3]| std::array::from_fn(|i| p[i] + offset[i]);
            let result = entry(
                shift(from),
                shift(to),
                Bounds {
                    min: shift(BOX.min),
                    max: shift(BOX.max),
                },
                3.,
                5.,
            );
            assert!((result.unwrap() - 2.7).abs() < 1e-12);
        }
    }
}
