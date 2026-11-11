//! Native box bounds derived from the world's single logical lid-progress owner.
use assets::registry::java::physics::Bounds;

/// Native getBoundingBox followed by Shapes.create, without shell render scaling.
pub fn outline(progress: f32, facing: u8) -> Option<Bounds> {
    if !progress.is_finite() || !(0. ..=1.).contains(&progress) {
        return None;
    }
    let step = world::java::piston::step(facing)?;
    let mut bounds = Bounds {
        min: [0.; 3],
        max: [1.; 3],
    };
    let extension = f64::from(0.5 * progress);
    for i in 0..3 {
        if step[i] > 0 {
            let end = 1. + extension;
            if end - 1. >= super::collision::CONTACT {
                bounds.max[i] = end;
            }
        }
        if step[i] < 0 && extension >= super::collision::CONTACT {
            bounds.min[i] = -extension;
        }
    }
    Some(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_follow_native_tick_progress_in_all_six_directions() {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../../../world/tests/fixtures/java-shulker-progress.json"
        ))
        .unwrap();
        let mut motion = world::java::shulker::Motion::default();
        for row in data["ticks"].as_array().unwrap() {
            let event = row["event"].as_i64().unwrap();
            if event >= 0 {
                motion.event(event as u8);
            }
            motion.tick();
            for facing in 0..6 {
                let actual = outline(motion.sample(1.), facing).unwrap();
                let expected = &row["bounds"][facing as usize];
                assert_eq!(
                    actual.min.map(f64::to_bits),
                    std::array::from_fn(|i| expected[i].as_f64().unwrap().to_bits()),
                    "{row}"
                );
                assert_eq!(
                    actual.max.map(f64::to_bits),
                    std::array::from_fn(|i| expected[i + 3].as_f64().unwrap().to_bits()),
                    "{row}"
                );
            }
        }
    }
}
