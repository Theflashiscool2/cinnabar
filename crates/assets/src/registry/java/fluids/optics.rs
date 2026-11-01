//! Functional light-transfer facts exported by the pinned native LightEngine API.
use serde::Deserialize;
#[derive(Clone, Debug, Deserialize)]
pub struct Optics {
    pub dampening: u8,
    pub faces: [Vec<[f32; 4]>; 6],
}
impl Optics {
    pub(super) fn valid(&self) -> bool {
        self.dampening <= 15
            && self
                .faces
                .iter()
                .flatten()
                .all(|r| r.iter().all(|v| v.is_finite()) && r[0] < r[2] && r[1] < r[3])
    }
    /// Native faceShapeOccludes: the union must cover the unit face, not just one rectangle.
    pub fn occludes(&self, other: &Self, direction: usize) -> bool {
        let faces = self.faces[direction]
            .iter()
            .chain(&other.faces[direction ^ 1]);
        let rectangles: Vec<_> = faces.collect();
        if rectangles.is_empty() {
            return false;
        }
        let axes: [Vec<f32>; 2] = std::array::from_fn(|axis| {
            let mut v = vec![0., 1.];
            for r in &rectangles {
                v.extend([r[axis], r[axis + 2]].map(|x| x.clamp(0., 1.)));
            }
            v.sort_by(f32::total_cmp);
            v.dedup();
            v
        });
        axes[0].windows(2).all(|x| {
            axes[1].windows(2).all(|y| {
                let p = [(x[0] + x[1]) * 0.5, (y[0] + y[1]) * 0.5];
                rectangles
                    .iter()
                    .any(|r| r[0] <= p[0] && p[0] <= r[2] && r[1] <= p[1] && p[1] <= r[3])
            })
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complementary_partial_faces_occlude_but_overlapping_halves_leave_a_gap() {
        let mut a = Optics {
            dampening: 0,
            faces: Default::default(),
        };
        let mut b = a.clone();
        a.faces[0] = vec![[0., 0., 0.5, 1.]];
        b.faces[1] = vec![[0.5, 0., 1., 1.]];
        assert!(a.occludes(&b, 0));
        assert!(b.occludes(&a, 1));
        b.faces[1] = vec![[0.25, 0., 0.75, 1.]];
        assert!(!a.occludes(&b, 0));
        a.faces[0] = vec![[0., 0., 0.5, 0.5], [0., 0.5, 0.5, 1.], [0.5, 0., 1., 1.]];
        assert!(a.occludes(
            &Optics {
                dampening: 0,
                faces: Default::default()
            },
            0
        ));
        assert!(!a.occludes(&b, 2));
    }
}

#[cfg(test)]
mod native_tests {
    use sha2::{Digest, Sha256};
    #[test]
    fn merged_faces_match_pinned_native_partial_and_transparent_state_pairs() {
        let (Ok(root), Ok(reference)) = (
            std::env::var("CINNABAR_JAVA_TEST_ASSETS"),
            std::env::var("CINNABAR_JAVA_LIGHT_FACES"),
        ) else {
            eprintln!("missing fixture: CINNABAR_JAVA_TEST_ASSETS and CINNABAR_JAVA_LIGHT_FACES");
            return;
        };
        let bytes = std::fs::read(std::path::Path::new(&root).join("fluids.json")).unwrap();
        let count = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["states"]
            .as_array()
            .unwrap()
            .len();
        let table =
            super::super::Table::load(&bytes, &format!("{:x}", Sha256::digest(&bytes)), count)
                .unwrap();
        assert!(table.has_lighting());
        let text = std::fs::read_to_string(reference).unwrap();
        let mut lines = text.lines();
        let n: usize = lines
            .next()
            .unwrap()
            .strip_prefix("COUNT ")
            .unwrap()
            .parse()
            .unwrap();
        let names = super::super::FACES.map(|face| format!("{face:?}").to_ascii_lowercase());
        let mut checked = 0;
        for line in lines {
            let v: Vec<_> = line.split_whitespace().collect();
            assert_eq!(v.len(), 4);
            let a = table
                .state(v[0].parse().unwrap())
                .unwrap()
                .light
                .as_ref()
                .unwrap();
            let b = table
                .state(v[1].parse().unwrap())
                .unwrap()
                .light
                .as_ref()
                .unwrap();
            let direction = names.iter().position(|s| s == v[2]).unwrap();
            assert_eq!(
                a.occludes(b, direction),
                v[3].parse::<bool>().unwrap(),
                "{line}"
            );
            checked += 1;
        }
        assert_eq!(checked, n * n * 6);
        eprintln!("matched {checked} native face pairs across {n} states");
    }
}
