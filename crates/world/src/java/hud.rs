//! Native player status facts. Presentation reads these without packet or Bedrock identifiers.

/// The pinned vanilla FoodData range; independent of its current server-supplied value.
pub const FOOD_POINTS: u16 = 20;
/// Ten armor icons display twenty points; values above this remain in authoritative attributes.
pub const DISPLAY_ARMOR_POINTS: u16 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}
impl GameMode {
    pub fn survival_stats(self) -> bool {
        matches!(self, Self::Survival | Self::Adventure)
    }
    pub fn shows_hotbar(self) -> bool {
        self != Self::Spectator
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Experience {
    pub progress: f32,
    pub level: u32,
    pub total: u32,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub mode: Option<GameMode>,
    pub experience: Option<Experience>,
    pub unimplemented_notifications: u64,
    pub odd_values: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub session: u64,
    pub epoch: u64,
    pub mode: Option<GameMode>,
    pub hardcore: bool,
    pub health: Option<f32>,
    pub maximum_health: Option<f32>,
    pub food: Option<i32>,
    pub saturation: Option<f32>,
    pub armor: Option<f64>,
    pub experience: Option<Experience>,
}
