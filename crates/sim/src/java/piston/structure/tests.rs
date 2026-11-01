use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn stone() -> Facts {
    Facts {
        reaction: Reaction::PushPull,
        air: false,
        block_entity: false,
        piston_base: false,
        extended: false,
        unbreakable: false,
        sticky: Sticky::None,
    }
}
#[derive(Default)]
struct Fixture {
    blocks: BTreeMap<Position, Facts>,
    missing: Option<Position>,
}
impl Access for Fixture {
    fn block(&self, p: Position) -> Result<Facts, Error> {
        if self.missing == Some(p) {
            return Err(Error::MissingData(p));
        }
        Ok(*self.blocks.get(&p).unwrap_or(&Facts {
            air: true,
            ..stone()
        }))
    }
    fn within_border(&self, p: Position) -> bool {
        p[0].abs() < 30_000_000 && p[2].abs() < 30_000_000
    }
    fn height(&self) -> (i32, i32) {
        (-64, 319)
    }
    fn max_push_depth(&self) -> usize {
        12
    }
}
#[test]
fn twelve_block_limit_and_negative_chunk_boundary_keep_native_source_order() {
    let mut f = Fixture::default();
    for x in -1..=10 {
        f.blocks.insert([x, 37, -1], stone());
    }
    let plan = resolve(&f, [-2, 37, -1], 5, true).unwrap().unwrap();
    assert_eq!(
        plan.push,
        (-1..=10).map(|x| [x, 37, -1]).collect::<Vec<_>>()
    );
    assert!(plan.destroy.is_empty());
    assert_eq!(f.blocks.len(), 12);
    f.blocks.insert([11, 37, -1], stone());
    assert_eq!(resolve(&f, [-2, 37, -1], 5, true).unwrap(), None);
}
#[test]
fn popped_blocks_destroy_only_in_the_forward_path_and_not_during_retraction() {
    let mut f = Fixture::default();
    f.blocks.insert(
        [1, 37, 0],
        Facts {
            reaction: Reaction::Popped,
            block_entity: true,
            ..stone()
        },
    );
    let p = resolve(&f, [0, 37, 0], 5, true).unwrap().unwrap();
    assert!(p.push.is_empty());
    assert_eq!(p.destroy, [[1, 37, 0]]);
    f.blocks.insert(
        [2, 37, 0],
        Facts {
            reaction: Reaction::Popped,
            ..stone()
        },
    );
    assert_eq!(resolve(&f, [0, 37, 0], 5, false).unwrap(), None);
}
#[test]
fn slime_carries_side_blocks_but_not_honey_and_does_not_destroy_side_flowers() {
    let mut f = Fixture::default();
    f.blocks.insert(
        [1, 37, 0],
        Facts {
            sticky: Sticky::Slime,
            ..stone()
        },
    );
    f.blocks.insert([1, 37, -1], stone());
    f.blocks.insert(
        [1, 37, 1],
        Facts {
            sticky: Sticky::Honey,
            ..stone()
        },
    );
    f.blocks.insert(
        [1, 38, 0],
        Facts {
            reaction: Reaction::Popped,
            ..stone()
        },
    );
    let p = resolve(&f, [0, 37, 0], 5, true).unwrap().unwrap();
    assert_eq!(p.push, [[1, 37, 0], [1, 37, -1]]);
    assert!(p.destroy.is_empty());
}
#[test]
fn immovable_side_neighbors_do_not_prevent_slime_movement_but_forward_obstacles_do() {
    let mut f = Fixture::default();
    f.blocks.insert(
        [1, 37, 0],
        Facts {
            sticky: Sticky::Slime,
            ..stone()
        },
    );
    let blocked = Facts {
        reaction: Reaction::Immoveable,
        ..stone()
    };
    f.blocks.insert([1, 37, -1], blocked);
    assert_eq!(
        resolve(&f, [0, 37, 0], 5, true).unwrap().unwrap().push,
        [[1, 37, 0]]
    );
    f.blocks.insert([2, 37, 0], blocked);
    assert_eq!(resolve(&f, [0, 37, 0], 5, true).unwrap(), None);
}
#[test]
fn missing_neighbors_defer_without_mutation_then_recover_from_a_resident_snapshot() {
    let mut f = Fixture::default();
    f.blocks.insert([1, 37, 0], stone());
    f.missing = Some([2, 37, 0]);
    assert_eq!(
        resolve(&f, [0, 37, 0], 5, true),
        Err(Error::MissingData([2, 37, 0]))
    );
    assert_eq!(f.blocks.len(), 1);
    f.missing = None;
    assert_eq!(
        resolve(&f, [0, 37, 0], 5, true).unwrap().unwrap().push,
        [[1, 37, 0]]
    );
}
#[test]
fn overlapping_sticky_branches_are_unique_and_translation_invariant() {
    let mut f = Fixture::default();
    for p in [[1, 37, 0], [2, 37, 0], [1, 37, -1], [2, 37, -1]] {
        f.blocks.insert(
            p,
            Facts {
                sticky: Sticky::Slime,
                ..stone()
            },
        );
    }
    let plan = resolve(&f, [0, 37, 0], 5, true).unwrap().unwrap();
    assert_eq!(
        plan.push.iter().copied().collect::<BTreeSet<_>>(),
        f.blocks.keys().copied().collect()
    );
    assert_eq!(plan.push.len(), 4);
    assert!(plan.destroy.is_empty());
    let delta = [-17, 16, -32];
    let shift = |p: Position| std::array::from_fn(|i| p[i] + delta[i]);
    let translated = Fixture {
        blocks: f.blocks.iter().map(|(p, s)| (shift(*p), *s)).collect(),
        missing: None,
    };
    assert_eq!(
        resolve(&translated, shift([0, 37, 0]), 5, true)
            .unwrap()
            .unwrap()
            .push,
        plan.push.into_iter().map(shift).collect::<Vec<_>>()
    );
}
#[test]
fn retracting_push_only_blocks_are_not_pulled_and_vertical_limits_are_observed() {
    let mut f = Fixture::default();
    f.blocks.insert(
        [2, 37, 0],
        Facts {
            reaction: Reaction::Push,
            ..stone()
        },
    );
    assert_eq!(resolve(&f, [0, 37, 0], 5, false).unwrap(), None);
    f.blocks.insert([0, 319, 0], stone());
    assert_eq!(resolve(&f, [0, 318, 0], 1, true).unwrap(), None);
    assert_eq!(resolve(&f, [0, 37, 0], 6, true), Err(Error::InvalidContext));
}

#[test]
fn attached_support_floor_exceeding_the_limit_blocks_the_whole_slime_structure() {
    // Reproduced independently on the controlled vanilla server before removing the support.
    let mut f = Fixture::default();
    f.blocks.insert(
        [1, 37, 0],
        Facts {
            sticky: Sticky::Slime,
            ..stone()
        },
    );
    f.blocks.insert([1, 37, -1], stone());
    f.blocks.insert(
        [1, 37, 1],
        Facts {
            sticky: Sticky::Honey,
            ..stone()
        },
    );
    for x in 0..=28 {
        f.blocks.insert([x, 36, 0], stone());
    }
    assert_eq!(resolve(&f, [0, 37, 0], 5, true).unwrap(), None);
    f.blocks.remove(&[1, 36, 0]);
    assert_eq!(
        resolve(&f, [0, 37, 0], 5, true).unwrap().unwrap().push,
        [[1, 37, 0], [1, 37, -1]]
    );
}

#[test]
fn ordinary_retraction_starts_two_blocks_out_and_moves_towards_the_base() {
    let mut f = Fixture::default();
    f.blocks.insert([258, 37, 252], stone());
    assert_eq!(
        resolve(&f, [260, 37, 252], 4, false).unwrap().unwrap().push,
        [[258, 37, 252]]
    );
}
