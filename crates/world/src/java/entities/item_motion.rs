//! Native dropped-item visual kinematics, independent of authoritative motion and pickups.
use crate::java::player::Motion;

#[derive(Clone, Debug)]
pub struct View {
    pub previous: [f64; 3],
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub dimensions: [f32; 2],
    pub on_ground: bool,
    pub ticks: u32,
    /// False means explicit server-position fallback, not collision-free terrain.
    pub active: bool,
    pub fallback: Option<&'static str>,
}
impl View {
    pub fn new(motion: Motion, dimensions: [f32; 2]) -> Self {
        Self {
            previous: motion.position,
            position: motion.position,
            velocity: motion.velocity,
            dimensions,
            on_ground: false,
            ticks: 0,
            active: false,
            fallback: Some("awaiting native item data and physics"),
        }
    }
    pub fn reposition(&mut self, position: [f64; 3], on_ground: bool) {
        self.position = position;
        self.previous = position;
        self.on_ground = on_ground;
    }
    pub fn sample(&self, fraction: f32) -> [f64; 3] {
        let t = f64::from(fraction.clamp(0., 1.));
        std::array::from_fn(|i| self.previous[i] + (self.position[i] - self.previous[i]) * t)
    }
    pub fn suspend(&mut self, motion: Motion, on_ground: bool, reason: &'static str) {
        self.reposition(motion.position, on_ground);
        self.velocity = motion.velocity;
        self.active = false;
        self.fallback = Some(reason);
    }
}
