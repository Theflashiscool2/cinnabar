//! Java screen gestures: carried items act on release, before the server executes quick craft.
//! Behavior reference: target AbstractContainerScreen and the MIT reference's container_screen.
use super::{DOUBLE_CLICK_MILLIS, PointerAction, PointerFrame};
use ui_screens::container_actions::InventoryCellHit as Hit;

#[derive(Debug, Clone)]
struct Drag {
    secondary: bool,
    cells: Vec<Hit>,
}
#[derive(Debug, Clone, Default)]
pub(super) struct Pointer {
    drag: Option<Drag>,
    collect: Option<Hit>,
    last_primary: Option<(Hit, u64)>,
    skip_release: bool,
}
impl Pointer {
    pub(super) fn step(
        &mut self,
        f: PointerFrame,
        eligible: bool,
        carried_count: usize,
        capacity: usize,
    ) -> Vec<PointerAction> {
        let mut actions = Vec::new();
        if f.primary_pressed || f.secondary_pressed {
            if self.drag.is_some() || self.collect.is_some() {
                return actions;
            }
            self.skip_release = false;
            let Some(hit) = f.hit else {
                return actions;
            };
            let secondary = !f.primary_pressed;
            let double = !secondary
                && self.last_primary.is_some_and(|(old, at)| {
                    old == hit && f.now_millis.saturating_sub(at) <= DOUBLE_CLICK_MILLIS
                });
            self.last_primary = (!secondary && !f.shift).then_some((hit, f.now_millis));
            if double && f.holding && !f.shift {
                self.last_primary = None;
                self.collect = Some(hit);
            } else if f.holding {
                self.drag = Some(Drag {
                    secondary,
                    cells: if eligible && capacity > 0 {
                        vec![hit]
                    } else {
                        Vec::new()
                    },
                });
            } else {
                self.skip_release = true;
                actions.push(if f.shift {
                    PointerAction::QuickMove(hit)
                } else if secondary {
                    PointerAction::SecondaryClick(hit)
                } else {
                    PointerAction::Click(hit)
                });
            }
        }
        if let Some(drag) = &mut self.drag {
            if !f.holding {
                self.drag = None;
                return actions;
            }
            if eligible
                && let Some(hit) = f.hit
                && drag.cells.len() < capacity
                && drag.cells.len() < carried_count
                && !drag.cells.contains(&hit)
            {
                drag.cells.push(hit);
            }
        }
        if !f.primary_released && !f.secondary_released {
            return actions;
        }
        if self.skip_release {
            self.skip_release = false;
            return actions;
        }
        if let Some(hit) = self.collect.take() {
            if f.primary_released && f.hit == Some(hit) && f.holding {
                actions.push(PointerAction::Gather {
                    hit,
                    shift: false,
                    focus: None,
                });
            }
            return actions;
        }
        let Some(drag) = self.drag.take() else {
            return actions;
        };
        if drag.secondary != f.secondary_released || !f.holding {
            return actions;
        }
        match drag.cells.as_slice() {
            [] => {
                if let Some(hit) = f.hit {
                    actions.push(if f.shift {
                        PointerAction::QuickMove(hit)
                    } else if drag.secondary {
                        PointerAction::SecondaryClick(hit)
                    } else {
                        PointerAction::Click(hit)
                    });
                }
            }
            [hit] => actions.push(if drag.secondary {
                PointerAction::SecondaryClick(*hit)
            } else {
                PointerAction::Click(*hit)
            }),
            _ => actions.push(PointerAction::Distribute {
                cells: drag.cells,
                one_each: drag.secondary,
            }),
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: Hit = Hit::Player(9);
    const B: Hit = Hit::Player(10);
    fn frame(hit: Option<Hit>, at: u64) -> PointerFrame {
        PointerFrame {
            primary_pressed: false,
            primary_released: false,
            secondary_pressed: false,
            secondary_released: false,
            shift: false,
            holding: true,
            hit,
            hit_stack: None,
            now_millis: at,
        }
    }
    #[test]
    fn carried_drag_is_one_release_action_and_never_places_on_press() {
        for secondary in [false, true] {
            let mut p = Pointer::default();
            assert!(
                p.step(
                    PointerFrame {
                        primary_pressed: !secondary,
                        secondary_pressed: secondary,
                        ..frame(Some(A), 0)
                    },
                    true,
                    11,
                    46
                )
                .is_empty()
            );
            assert!(p.step(frame(Some(B), 20), true, 11, 46).is_empty());
            assert_eq!(
                p.step(
                    PointerFrame {
                        primary_released: !secondary,
                        secondary_released: secondary,
                        ..frame(Some(B), 30)
                    },
                    true,
                    11,
                    46
                ),
                vec![PointerAction::Distribute {
                    cells: vec![A, B],
                    one_each: secondary
                }]
            );
            assert!(p.step(frame(Some(A), 40), true, 11, 46).is_empty());
        }
    }
    #[test]
    fn single_cell_and_incompatible_swap_are_normal_release_pickups() {
        for eligible in [false, true] {
            let mut p = Pointer::default();
            assert!(
                p.step(
                    PointerFrame {
                        primary_pressed: true,
                        ..frame(Some(A), 0)
                    },
                    eligible,
                    5,
                    46
                )
                .is_empty()
            );
            assert_eq!(
                p.step(
                    PointerFrame {
                        primary_released: true,
                        ..frame(Some(A), 10)
                    },
                    eligible,
                    5,
                    46
                ),
                vec![PointerAction::Click(A)]
            );
        }
    }
    #[test]
    fn empty_cursor_pickup_and_shift_have_no_duplicate_release() {
        let mut p = Pointer::default();
        assert_eq!(
            p.step(
                PointerFrame {
                    primary_pressed: true,
                    holding: false,
                    ..frame(Some(A), 0)
                },
                false,
                0,
                46
            ),
            vec![PointerAction::Click(A)]
        );
        assert!(
            p.step(
                PointerFrame {
                    primary_released: true,
                    ..frame(Some(A), 10)
                },
                true,
                10,
                46
            )
            .is_empty()
        );
        assert!(
            p.step(
                PointerFrame {
                    primary_pressed: true,
                    ..frame(Some(A), 50)
                },
                true,
                10,
                46
            )
            .is_empty()
        );
        assert_eq!(
            p.step(
                PointerFrame {
                    primary_released: true,
                    ..frame(Some(A), 60)
                },
                true,
                10,
                46
            ),
            vec![PointerAction::Gather {
                hit: A,
                shift: false,
                focus: None
            }]
        );
    }
    #[test]
    fn a_click_after_collect_places_normally_instead_of_recollecting_forever() {
        let mut p = Pointer::default();
        p.step(
            PointerFrame {
                primary_pressed: true,
                holding: false,
                ..frame(Some(A), 0)
            },
            false,
            0,
            46,
        );
        p.step(
            PointerFrame {
                primary_released: true,
                ..frame(Some(A), 5)
            },
            true,
            11,
            46,
        );
        p.step(
            PointerFrame {
                primary_pressed: true,
                ..frame(Some(A), 10)
            },
            true,
            11,
            46,
        );
        assert_eq!(
            p.step(
                PointerFrame {
                    primary_released: true,
                    ..frame(Some(A), 15)
                },
                true,
                11,
                46
            ),
            vec![PointerAction::Gather {
                hit: A,
                shift: false,
                focus: None
            }]
        );
        assert!(
            p.step(
                PointerFrame {
                    primary_pressed: true,
                    ..frame(Some(A), 20)
                },
                true,
                11,
                46
            )
            .is_empty()
        );
        assert_eq!(
            p.step(
                PointerFrame {
                    primary_released: true,
                    ..frame(Some(A), 25)
                },
                true,
                11,
                46
            ),
            vec![PointerAction::Click(A)]
        );
    }
    #[test]
    fn a_complete_tap_in_one_frame_still_places_once() {
        let mut p = Pointer::default();
        assert_eq!(
            p.step(
                PointerFrame {
                    primary_pressed: true,
                    primary_released: true,
                    ..frame(Some(A), 0)
                },
                true,
                5,
                46
            ),
            vec![PointerAction::Click(A)]
        );
        assert!(
            p.step(
                PointerFrame {
                    primary_released: true,
                    ..frame(Some(A), 10)
                },
                true,
                5,
                46
            )
            .is_empty()
        );
    }
    #[test]
    fn duplicate_cells_wrong_release_bounds_and_retirement_are_safe() {
        let mut p = Pointer::default();
        p.step(
            PointerFrame {
                secondary_pressed: true,
                ..frame(Some(A), 0)
            },
            true,
            1,
            46,
        );
        p.step(frame(Some(A), 1), true, 1, 46);
        p.step(frame(Some(B), 2), true, 1, 46);
        assert!(
            p.step(
                PointerFrame {
                    primary_released: true,
                    ..frame(Some(B), 3)
                },
                true,
                1,
                46
            )
            .is_empty()
        );
        p.step(
            PointerFrame {
                primary_pressed: true,
                ..frame(Some(A), 400)
            },
            true,
            10,
            46,
        );
        p = Pointer::default();
        assert!(
            p.step(
                PointerFrame {
                    primary_released: true,
                    ..frame(Some(B), 410)
                },
                true,
                10,
                46
            )
            .is_empty()
        );
    }
}
