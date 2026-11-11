//! Native ShulkerBoxBlockEntity lid progress, advanced only by the logical world tick.
const PROGRESS_PER_TICK: f32 = 0.1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Status {
    #[default]
    Closed,
    Opening,
    Opened,
    Closing,
}
#[derive(Clone, Debug, Default)]
pub struct Motion {
    old: f32,
    current: f32,
    status: Status,
}
impl Motion {
    /// Native event one changes direction only for viewer counts zero and one.
    pub fn event(&mut self, viewers: u8) {
        match viewers {
            0 => self.status = Status::Closing,
            1 => self.status = Status::Opening,
            _ => {}
        }
    }
    pub fn tick(&mut self) {
        self.old = self.current;
        match self.status {
            Status::Closed => self.current = 0.,
            Status::Opened => self.current = 1.,
            Status::Opening => {
                self.current += PROGRESS_PER_TICK;
                if self.current >= 1. {
                    self.current = 1.;
                    self.status = Status::Opened;
                }
            }
            Status::Closing => {
                self.current -= PROGRESS_PER_TICK;
                if self.current <= 0. {
                    self.current = 0.;
                    self.status = Status::Closed;
                }
            }
        }
    }
    /// Collision/picking pass one; presentation supplies its existing timer fraction.
    pub fn sample(&self, partial: f32) -> f32 {
        self.old + partial.clamp(0., 1.) * (self.current - self.old)
    }
}
#[derive(Clone, Debug)]
pub struct State {
    pub generation: u64,
    pub block_id: u32,
    pub states: std::sync::Arc<[u32]>,
    pub motion: Motion,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_matches_official_tick_and_viewer_count_sequences() {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/java-shulker-progress.json"
        ))
        .unwrap();
        let mut motion = Motion::default();
        for row in data["ticks"].as_array().unwrap() {
            let event = row["event"].as_i64().unwrap();
            if event >= 0 {
                motion.event(event as u8);
            }
            motion.tick();
            for (i, partial) in [0., 0.25, 0.5, 1.].into_iter().enumerate() {
                assert_eq!(
                    motion.sample(partial).to_bits(),
                    (row["samples"][i].as_f64().unwrap() as f32).to_bits(),
                    "{row}"
                );
            }
        }
    }
}
