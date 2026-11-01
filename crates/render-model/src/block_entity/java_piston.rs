//! Native moving-block model selection, separate from the moving-piston host's empty model.
//! Reference: Java 26.3 PistonHeadRenderer.extractRenderState, not block model filenames.
use assets::registry::java::{
    BlockRegistry,
    piston::{BASE, HEAD, STICKY_BASE},
};
use std::collections::BTreeMap;
use world::java::piston::{DEFAULT_MOVED_BLOCK, Snapshot, step};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Ordinary weighted/multipart model selected at `sampling_offset`, then translated.
    pub moving: Option<u32>,
    /// A retracting source also draws its extended base at the entity position, with light
    /// and model seed sampled there. It shares the moving part's biome snapshot.
    pub base: Option<u32>,
    /// Moving-part light, biome and model-seed position relative to the entity position.
    pub sampling_offset: [i32; 3],
}

impl Plan {
    pub fn resolve(
        registry: &BlockRegistry,
        data: &Snapshot,
        progress: f32,
    ) -> Result<Self, &'static str> {
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err("invalid Java piston render progress");
        }
        let direction = step(data.facing).ok_or("invalid Java piston render direction")?;
        let sampling_offset = direction.map(|n| if data.extending { -n } else { n });
        let select = |name: &str, properties: &BTreeMap<String, String>| {
            registry
                .state_id(name, properties)
                .ok_or("unresolved Java piston moved state")
        };
        let moved = select(&data.name, &data.properties)?;
        if data.name == DEFAULT_MOVED_BLOCK {
            return Ok(Self {
                moving: None,
                base: None,
                sampling_offset,
            });
        }
        let mut properties = registry
            .state(moved)
            .ok_or("missing Java piston moved state")?
            .properties
            .clone();
        let (moving, base) = if data.name == HEAD {
            properties.insert("short".into(), (progress <= 0.5).to_string());
            (select(&data.name, &properties)?, None)
        } else if data.source && !data.extending {
            if !matches!(data.name.as_str(), BASE | STICKY_BASE) {
                return Err("Java retracting source must contain a piston base");
            }
            let head = BTreeMap::from([
                (
                    "facing".into(),
                    properties
                        .get("facing")
                        .ok_or("missing Java piston facing")?
                        .clone(),
                ),
                (
                    "type".into(),
                    if data.name == STICKY_BASE {
                        "sticky"
                    } else {
                        "normal"
                    }
                    .into(),
                ),
                ("short".into(), (progress >= 0.5).to_string()),
            ]);
            properties.insert("extended".into(), "true".into());
            (select(HEAD, &head)?, Some(select(&data.name, &properties)?))
        } else {
            (moved, None)
        };
        Ok(Self {
            moving: Some(moving),
            base,
            sampling_offset,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    fn fixture() -> Option<BlockRegistry> {
        let Ok(path) = std::env::var("CINNABAR_JAVA_PISTON_BLOCKS") else {
            eprintln!("missing fixture: CINNABAR_JAVA_PISTON_BLOCKS (pinned blocks.json)");
            return None;
        };
        let bytes = std::fs::read(path).unwrap();
        Some(BlockRegistry::load(&bytes, &format!("{:x}", Sha256::digest(&bytes))).unwrap())
    }
    fn snapshot(name: &str, extending: bool, source: bool) -> Snapshot {
        Snapshot {
            name: name.into(),
            properties: BTreeMap::from([("facing".into(), "east".into())]),
            facing: 5,
            progress: 0.,
            extending,
            source,
        }
    }
    #[test]
    fn native_head_threshold_and_retracting_base_are_separate_models() {
        let Some(registry) = fixture() else {
            return;
        };
        let mut data = snapshot("minecraft:piston_head", true, true);
        data.properties.insert("type".into(), "normal".into());
        for (progress, short) in [
            (0., "true"),
            (0.5, "true"),
            (0.5001, "false"),
            (1., "false"),
        ] {
            let p = Plan::resolve(&registry, &data, progress).unwrap();
            assert_eq!(
                registry.state(p.moving.unwrap()).unwrap().properties["short"],
                short
            );
            assert_eq!(p.base, None);
            assert_eq!(p.sampling_offset, [-1, 0, 0]);
        }
        for name in ["minecraft:piston", "minecraft:sticky_piston"] {
            let data = snapshot(name, false, true);
            for (progress, short) in [
                (0., "false"),
                (0.4999, "false"),
                (0.5, "true"),
                (1., "true"),
            ] {
                let p = Plan::resolve(&registry, &data, progress).unwrap();
                let head = registry.state(p.moving.unwrap()).unwrap();
                let base = registry.state(p.base.unwrap()).unwrap();
                assert_eq!(head.name.as_ref(), "minecraft:piston_head");
                assert_eq!(head.properties["short"], short);
                assert_eq!(
                    head.properties["type"],
                    if name == "minecraft:piston" {
                        "normal"
                    } else {
                        "sticky"
                    }
                );
                assert_eq!(head.properties["facing"], "east");
                assert_eq!(base.name.as_ref(), name);
                assert_eq!(base.properties["extended"], "true");
                assert_eq!(p.sampling_offset, [1, 0, 0]);
            }
        }
    }
    #[test]
    fn ordinary_partial_models_keep_properties_and_unknown_states_fail_explicitly() {
        let Some(registry) = fixture() else {
            return;
        };
        let mut data = snapshot("minecraft:oak_stairs", true, false);
        data.properties.insert("half".into(), "top".into());
        let plan = Plan::resolve(&registry, &data, 0.25).unwrap();
        let moved = registry.state(plan.moving.unwrap()).unwrap();
        assert_eq!(moved.properties["half"], "top");
        assert_eq!(moved.properties["facing"], "east");
        assert!(plan.base.is_none());
        data.name = "minecraft:unknown_not_air".into();
        assert!(Plan::resolve(&registry, &data, 0.).is_err());
    }
}
