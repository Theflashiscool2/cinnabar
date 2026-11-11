//! One elapsed clock and input snapshot follow the loaned simulation across frame boundaries.
use std::time::Duration;
use web_time::Instant;
#[derive(Default)]
pub(in crate::session) struct ServiceState {
    clock: Option<Instant>,
    scope: Option<(u64, u64)>,
    pub(in crate::session) controls: gameplay::java::Controls,
    pub(in crate::session) ticks: u64,
    pub(in crate::session) corrected: bool,
    pub(super) reported_block: Option<String>,
}
impl ServiceState {
    pub(super) fn elapsed(&mut self, scope: Option<(u64, u64)>, now: Instant) -> Duration {
        if self.scope != scope {
            *self = Self {
                scope,
                ..Default::default()
            };
        }
        self.clock
            .replace(now)
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_frame_consumes_only_time_not_already_consumed_by_the_service() {
        let mut state = ServiceState::default();
        let start = Instant::now();
        let scope = Some((7, 1));
        assert_eq!(state.elapsed(scope, start), Duration::ZERO);
        assert_eq!(
            state.elapsed(scope, start + Duration::from_millis(50)),
            Duration::from_millis(50)
        );
        assert_eq!(
            state.elapsed(scope, start + Duration::from_millis(67)),
            Duration::from_millis(17)
        );
        assert_eq!(
            state.elapsed(scope, start + Duration::from_millis(67)),
            Duration::ZERO
        );
    }
    #[test]
    fn replacement_and_reconnect_retire_clock_input_and_pending_camera_receipts() {
        let mut state = ServiceState::default();
        let start = Instant::now();
        state.elapsed(Some((7, 1)), start);
        state.controls.buttons = 127;
        state.ticks = 91;
        state.corrected = true;
        assert_eq!(
            state.elapsed(Some((7, 2)), start + Duration::from_secs(5)),
            Duration::ZERO
        );
        assert_eq!(state.controls.buttons, 0);
        assert_eq!(state.ticks, 0);
        assert!(!state.corrected);
        state.controls.buttons = 31;
        assert_eq!(
            state.elapsed(Some((8, 1)), start + Duration::from_secs(7)),
            Duration::ZERO
        );
        assert_eq!(state.controls.buttons, 0);
        assert_eq!(
            state.elapsed(None, start + Duration::from_secs(8)),
            Duration::ZERO
        );
    }
}
