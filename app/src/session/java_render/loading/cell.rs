//! A camera outside build height has no section to acknowledge. Native26.3's
//! WaitingForPlayerChunk explicitly permits that case. Use the nearest real
//! vertical section for Cinnabar's current-generation renderer receipt; player
//! motion remains untouched and no geometry/receipt is fabricated.
pub(super) fn renderer_cell(d: &world::java::Dimension, position: [f64; 3]) -> Option<[i32; 3]> {
    if d.height <= 0 || position.iter().any(|n| !n.is_finite()) {
        return None;
    }
    let mut cell = [0; 3];
    for (out, n) in cell.iter_mut().zip(position) {
        let n = n.floor();
        if n < i32::MIN as f64 || n > i32::MAX as f64 {
            return None;
        }
        *out = n as i32;
    }
    let top = i32::try_from(i64::from(d.min_y) + i64::from(d.height) - 1).ok()?;
    cell[1] = cell[1].clamp(d.min_y, top);
    Some(cell)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dimension(min_y: i32, height: i32) -> world::java::Dimension {
        world::java::Dimension {
            session: 7,
            epoch: 2,
            name: "fixture:world".into(),
            min_y,
            height,
            has_skylight: false,
        }
    }
    #[test]
    fn below_above_and_edge_spawns_keep_xz_and_choose_only_a_registered_section() {
        let nether = dimension(0, 256);
        for (y, expected) in [
            (-60., 0),
            (-0.01, 0),
            (0., 0),
            (255.99, 255),
            (256., 255),
            (1024., 255),
        ] {
            assert_eq!(
                renderer_cell(&nether, [-0.5, y, -16.5]),
                Some([-1, expected, -17])
            );
        }
        let overworld = dimension(-64, 384);
        for (y, expected) in [
            (-65., -64),
            (-64., -64),
            (-16.01, -17),
            (319.99, 319),
            (320., 319),
        ] {
            assert_eq!(
                renderer_cell(&overworld, [31.5, y, 32.5]),
                Some([31, expected, 32])
            );
        }
    }
    #[test]
    fn invalid_coordinates_or_dimensions_never_become_a_ready_cell() {
        let d = dimension(0, 256);
        for p in [
            [f64::NAN, 0., 0.],
            [0., f64::INFINITY, 0.],
            [0., 0., f64::NEG_INFINITY],
            [i32::MAX as f64 + 1., 0., 0.],
        ] {
            assert_eq!(renderer_cell(&d, p), None);
        }
        assert_eq!(renderer_cell(&dimension(0, 0), [0.; 3]), None);
        assert_eq!(renderer_cell(&dimension(i32::MAX, 16), [0.; 3]), None);
    }
}
