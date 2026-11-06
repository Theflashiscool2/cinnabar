//! Native villager clothing layers share the actor atlas, rig publisher and world lifetime.
use super::*;
use world::java::entities::{Entity, animation::Sample, visual::VillagerAppearance};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Hat {
    #[default]
    None,
    Partial,
    Full,
}
impl Hat {
    pub fn read(value: Option<&serde_json::Value>) -> Result<Self, &'static str> {
        match value
            .and_then(|v| v.get("villager"))
            .and_then(|v| v.get("hat"))
        {
            None => Ok(Self::None),
            Some(serde_json::Value::String(s)) if s == "none" => Ok(Self::None),
            Some(serde_json::Value::String(s)) if s == "partial" => Ok(Self::Partial),
            Some(serde_json::Value::String(s)) if s == "full" => Ok(Self::Full),
            _ => Err("invalid native villager hat metadata"),
        }
    }
}
pub(super) struct Pose {
    model: Arc<render_model::actor::java_geometry::Model>,
    angles: [[f32; 3]; 3],
    bones: Arc<[RenderBoneTransform]>,
    bounds: ::assets::SkinGeometryBounds,
}
fn resource(folder: &str, id: &str) -> Result<String, String> {
    let id = ::assets::registry::java::source::Identifier::parse(id).map_err(|e| e.to_string())?;
    Ok(format!(
        "assets/{}/textures/entity/villager/{folder}/{}.png",
        id.namespace(),
        id.path()
    ))
}
/// Native type head is omitted only when its profession hat covers it.
fn type_has_head(profession: Hat, variant: Hat) -> bool {
    profession == Hat::None || (profession == Hat::Partial && variant != Hat::Full)
}
fn clothing(
    appearance: &VillagerAppearance,
    baby: bool,
    hats: &BTreeMap<String, Hat>,
) -> Result<Vec<(bool, String)>, String> {
    let mut layers = vec![(
        true,
        format!(
            "assets/minecraft/textures/entity/villager/villager{}.png",
            if baby { "_baby" } else { "" }
        ),
    )];
    let kind = resource("type", &appearance.variant)?;
    let profession = resource("profession", &appearance.profession)?;
    let head = type_has_head(
        hats.get(&profession).copied().unwrap_or_default(),
        hats.get(&kind).copied().unwrap_or_default(),
    );
    layers.push((
        head,
        resource(if baby { "baby" } else { "type" }, &appearance.variant)?,
    ));
    if !baby && appearance.profession.as_ref() != java_protocol::target::VILLAGER_DEFAULT_PROFESSION
    {
        layers.push((true, profession));
        if appearance.profession.as_ref() != "minecraft:nitwit" {
            // Pinned VillagerProfessionLayer LEVEL_LOCATIONS, clamped only for rendering.
            let levels = java_protocol::target::VILLAGER_LEVELS;
            let level = levels[appearance.level.clamp(1, levels.len() as i32) as usize - 1];
            layers.push((true, resource("profession_level", level)?));
        }
    }
    Ok(layers)
}
#[allow(
    clippy::too_many_arguments,
    reason = "binds native world facts to existing shared actor publication"
)]
pub(super) fn append(
    batch: &mut ActorPresentationBatch,
    entity: &Entity,
    appearance: &VillagerAppearance,
    sampled: Sample,
    assets: &assets::Prepared,
    poses: &mut BTreeMap<(Identity, bool), Pose>,
    frame: ground::Frame,
) -> Result<(), String> {
    let angles = sampled.villager_rotations(appearance.unhappy_ticks > 0);
    let matrix = Mat4::from_scale_rotation_translation(
        Vec3::splat(entity.visual.scale),
        Quat::from_rotation_y(-sampled.body_yaw.to_radians()),
        Vec3::from_array(sampled.pose.position.map(|v| v as f32)),
    )
    .to_cols_array_2d();
    for (layer, (head, texture)) in clothing(appearance, entity.visual.baby, &assets.villager_hats)?
        .into_iter()
        .enumerate()
    {
        let name = match (entity.visual.baby, head) {
            (false, true) => "villager",
            (false, false) => "villager_no_hat",
            (true, true) => "baby_villager",
            (true, false) => "baby_villager_no_hat",
        };
        let model = assets
            .models
            .get(name)
            .ok_or_else(|| format!("unprepared native model {name}"))?;
        let artwork = assets
            .player_textures
            .get(&texture)
            .copied()
            .ok_or_else(|| format!("unprepared native villager texture {texture}"))?;
        let key = (entity.identity, head);
        if poses
            .get(&key)
            .is_none_or(|pose| !Arc::ptr_eq(&pose.model, model) || pose.angles != angles)
        {
            let rotations = ["root/head", "root/right_leg", "root/left_leg"]
                .into_iter()
                .zip(angles)
                .map(|(name, angle)| {
                    model
                        .bone(name)
                        .map(|bone| (bone, angle))
                        .ok_or_else(|| format!("native villager bone missing: {name}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let bones = model.pose(&rotations).map_err(|e| e.to_string())?;
            let bounds = model.posed_bounds(&bones).map_err(|e| e.to_string())?;
            poses.insert(
                key,
                Pose {
                    model: Arc::clone(model),
                    angles,
                    bones,
                    bounds,
                },
            );
        }
        let pose = &poses[&key];
        let identity = ActorRenderIdentity {
            session_id: frame.scope.0,
            dimension: 0,
            runtime_id: entity.identity.incarnation,
            spawn_revision: std::num::NonZeroU64::new(entity.identity.incarnation)
                .ok_or("native villager incarnation missing")?,
            ingress_sequence: entity.revision,
            source_tick: None,
            movement_revision: entity.revision,
            pose_generation: render_api::PoseGeneration::new(frame.serial),
            layer: u8::try_from(layer).map_err(|e| e.to_string())?,
        };
        batch.submissions.push(ActorRigSubmission {
            material: ActorMaterial::default(),
            culling_bounds: pose.bounds,
            input: ActorRigRenderInput {
                identity,
                rig: model.geometry.id,
                previous_bones: Arc::clone(&pose.bones),
                current_bones: Arc::clone(&pose.bones),
                completed_tick: frame.serial,
                reset_generation: frame.scope.1,
            },
            world_from_actor: std::array::from_fn(|r| {
                [matrix[0][r], matrix[1][r], matrix[2][r], matrix[3][r]]
            }),
            texture_layer: artwork.layer,
            route: ActorRigRoute::Compiled,
            tint: 0,
            overlay_rgba8: 0,
            uv_anim: [0., 0., 1., 1.],
            light: frame.light,
        });
        batch.artwork.insert(identity, artwork);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_clothing_order_baby_profession_levels_and_hat_coverage() {
        let mut a = VillagerAppearance {
            variant: "minecraft:snow".into(),
            profession: "minecraft:farmer".into(),
            level: 999,
            unhappy_ticks: 0,
            finalized: false,
        };
        let hats = BTreeMap::from([
            (resource("type", &a.variant).unwrap(), Hat::Full),
            (resource("profession", &a.profession).unwrap(), Hat::Full),
        ]);
        let layers = clothing(&a, false, &hats).unwrap();
        assert_eq!(layers.len(), 4);
        assert!(layers[0].1.ends_with("/villager.png"));
        assert!(!layers[1].0);
        assert!(layers[2].1.ends_with("/profession/farmer.png"));
        assert!(layers[3].1.ends_with("/profession_level/diamond.png"));
        let baby = clothing(&a, true, &hats).unwrap();
        assert_eq!(baby.len(), 2);
        assert!(baby[0].1.ends_with("/villager_baby.png"));
        assert!(baby[1].1.ends_with("/baby/snow.png"));
        a.profession = "minecraft:nitwit".into();
        assert_eq!(clothing(&a, false, &hats).unwrap().len(), 3);
        a.profession = "minecraft:none".into();
        assert_eq!(clothing(&a, false, &hats).unwrap().len(), 2);
        assert!(type_has_head(Hat::Partial, Hat::None));
        assert!(!type_has_head(Hat::Partial, Hat::Full));
        assert!(Hat::read(Some(&serde_json::json!({"villager":{"hat":"invalid"}}))).is_err());
    }
}
