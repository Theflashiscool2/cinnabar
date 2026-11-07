//! Session/epoch-scoped border authority. Native border interpolation uses wall
//! time, not rendered frames or a second physics tick loop.
use java_protocol::session::border::Packet;
use std::time::Duration;
use web_time::Instant;
use world::java::border::Border;

#[derive(Default, Debug)]
pub struct Authority {
    scope: Option<(u64, u64)>,
    value: Option<Border>,
    size_changed: Option<Instant>,
    pub error: Option<&'static str>,
}
fn native_default() -> Result<Border, &'static str> {
    let d = assets::registry::java::use_rules::builtin()
        .map_err(|_| "Native border defaults are unavailable")?
        .border;
    Ok(Border::new(
        d.center,
        d.size,
        d.absolute_max,
        d.warning_blocks,
        d.warning_time,
    ))
}
impl Authority {
    pub(super) fn apply(&mut self, scope: (u64, u64), packet: Packet) -> bool {
        let result = self.apply_at(scope, packet, Instant::now());
        self.error = result.err();
        self.error.is_none()
    }
    fn apply_at(
        &mut self,
        scope: (u64, u64),
        packet: Packet,
        now: Instant,
    ) -> Result<(), &'static str> {
        if self.scope != Some(scope) {
            self.value = Some(native_default()?);
            self.scope = Some(scope);
            self.size_changed = None;
        }
        let value = self
            .value
            .as_mut()
            .ok_or("Native border state is unavailable")?;
        match packet {
            Packet::Initialize {
                center,
                old_size,
                new_size,
                millis,
                absolute_max,
                warning_blocks,
                warning_time,
            } => {
                *value = Border::new(center, old_size, absolute_max, warning_blocks, warning_time);
                value.lerp(old_size, new_size, millis);
                self.size_changed = Some(now);
            }
            Packet::Center(center) => value.center(center),
            Packet::Size(size) => {
                value.size(size);
                self.size_changed = None;
            }
            Packet::Lerp { from, to, millis } => {
                value.lerp(from, to, millis);
                self.size_changed = Some(now);
            }
            Packet::WarningBlocks(value_) => value.warning_blocks = value_,
            Packet::WarningTime(value_) => value.warning_time = value_,
        }
        Ok(())
    }
    pub fn contains_block(
        &self,
        scope: (u64, u64),
        position: [i32; 3],
    ) -> Result<bool, &'static str> {
        self.contains_at(scope, position, Instant::now())
    }
    fn contains_at(
        &self,
        scope: (u64, u64),
        position: [i32; 3],
        now: Instant,
    ) -> Result<bool, &'static str> {
        let (value, elapsed) = if self.scope == Some(scope) {
            (
                self.value.ok_or("Native border state is unavailable")?,
                self.size_changed
                    .map_or(Duration::ZERO, |t| now.saturating_duration_since(t)),
            )
        } else {
            (native_default()?, Duration::ZERO)
        };
        Ok(value.contains_block(position, elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authority_lerp_survives_center_updates_and_cannot_cross_world_scopes() {
        let start = Instant::now();
        let mut border = Authority::default();
        border
            .apply_at(
                (7, 1),
                Packet::Initialize {
                    center: [0., 0.],
                    old_size: 16.,
                    new_size: 8.,
                    millis: 10_000,
                    absolute_max: 100,
                    warning_blocks: 5,
                    warning_time: 15,
                },
                start,
            )
            .unwrap();
        let half = start + Duration::from_secs(5);
        assert!(!border.contains_at((7, 1), [6, 0, 0], half).unwrap());
        border
            .apply_at((7, 1), Packet::Center([1., 0.]), half)
            .unwrap();
        assert!(!border.contains_at((7, 1), [7, 0, 0], half).unwrap());
        assert!(border.contains_at((7, 2), [7, 0, 0], half).unwrap());
        assert!(border.contains_at((8, 1), [7, 0, 0], half).unwrap());
        border.apply_at((7, 2), Packet::Size(4.), half).unwrap();
        assert!(!border.contains_at((7, 2), [2, 0, 0], half).unwrap());
        assert!(border.contains_at((7, 2), [1, 0, 0], half).unwrap());
    }
    #[test]
    fn stale_border_packets_cannot_replace_current_world_authority() {
        use java_protocol::session::{Envelope, Event};
        let mut state = crate::java::WorldState::default();
        state.begin(7);
        state.terrain.begin(world::java::Dimension {
            session: 7,
            epoch: 2,
            name: "fixture".into(),
            min_y: 0,
            height: 256,
            has_skylight: true,
        });
        let update = |generation, dimension_epoch| Envelope {
            generation,
            event: Event::Border {
                dimension_epoch,
                packet: Packet::Size(2.),
            },
        };
        assert!(!state.admit(update(7, 1)));
        assert!(!state.admit(update(8, 2)));
        assert!(state.border.contains_block((7, 2), [10, 0, 0]).unwrap());
        assert!(state.admit(update(7, 2)));
        assert!(!state.border.contains_block((7, 2), [1, 0, 0]).unwrap());
        assert!(!state.admit(update(7, 1)));
        assert!(!state.border.contains_block((7, 2), [1, 0, 0]).unwrap());
    }
}
