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

/// Native ItemPickupParticle's frozen origin and tick-interpolated collector midpoint.
#[derive(Clone, Debug)]
pub struct Pickup {
    origin: [f64; 3],
    old_target: [f64; 3],
    target: [f64; 3],
    life: u8,
}
impl Pickup {
    pub const LIFETIME: u8 = 3;
    pub fn new(origin: [f64; 3], target: [f64; 3]) -> Self {
        Self {
            origin,
            old_target: target,
            target,
            life: 0,
        }
    }
    pub fn origin(&self) -> [f64; 3] {
        self.origin
    }
    /// Installs the local collector's actual eye midpoint before its first publication.
    pub fn initialize_target(&mut self, target: [f64; 3]) {
        if self.life == 0 {
            self.old_target = target;
            self.target = target;
        }
    }
    /// Returns false on the third tick, before another particle frame is submitted.
    pub fn tick(&mut self, target: [f64; 3]) -> bool {
        self.life += 1;
        self.old_target = self.target;
        self.target = target;
        self.life < Self::LIFETIME
    }
    pub fn sample(&self, fraction: f32) -> [f64; 3] {
        let fraction = fraction.clamp(0., 1.);
        let progress = ((f32::from(self.life) + fraction) / f32::from(Self::LIFETIME)).powi(2);
        std::array::from_fn(|axis| {
            let target = self.old_target[axis]
                + f64::from(fraction) * (self.target[axis] - self.old_target[axis]);
            self.origin[axis] + f64::from(progress) * (target - self.origin[axis])
        })
    }
}

#[cfg(test)]
mod pickup_tests {
    use super::*;
    #[test]
    fn native_pickup_quadratic_path_interpolates_target_and_expires_on_third_tick() {
        // ItemPickupParticleGroup.ParticleInstance.fromParticle in the pinned 26.3 client.
        let mut pickup = Pickup::new([0., 2., 4.], [9., 11., 13.]);
        assert_eq!(pickup.sample(0.), [0., 2., 4.]);
        let at_half = pickup.sample(0.5);
        for (actual, expected) in
            at_half
                .into_iter()
                .zip([0.2500000186264515, 2.2500000186264515, 4.2500000186264515])
        {
            assert!((actual - expected).abs() < 1e-14);
        }
        assert!(pickup.tick([18., 20., 22.]));
        assert_eq!(
            pickup.sample(0.),
            [1.000000074505806, 3.000000074505806, 5.000000074505806]
        );
        assert_eq!(pickup.sample(0.5), [3.375, 5.375, 7.375]);
        assert!(pickup.tick([18., 20., 22.]));
        assert!(!pickup.tick([18., 20., 22.]));
    }
}
