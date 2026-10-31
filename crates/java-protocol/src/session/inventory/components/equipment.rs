//! Equipment and predicate wire data; interaction/simulation owners remain authoritative.
use super::*;

pub(super) fn attack_range(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(
        json!({"min":r.f32()?,"max":r.f32()?,"creative_min":r.f32()?,"creative_max":r.f32()?,"hitbox_margin":r.f32()?,"mob_factor":r.f32()?}),
    )
}
pub(super) fn equippable(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(
        json!({"slot":nonnegative(r)?,"equip_sound":sound(r)?,"model":optional(r,key)?,
        "camera_overlay":optional(r,key)?,"allowed_entities":optional(r,holder_set)?,
        "dispensable":r.bool()?,"swappable":r.bool()?,"damage_on_hurt":r.bool()?,
        "equip_on_interact":r.bool()?,"shearable":r.bool()?,"shearing_sound":sound(r)?}),
    )
}
pub(super) fn blocks_attacks(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(json!({"delay_seconds":r.f32()?,"cooldown_scale":r.f32()?,
        "reductions":list(r,13,|r|Ok(json!({"angle":r.f32()?,"types":optional(r,holder_set)?,"base":r.f32()?,"factor":r.f32()?})))?,
        "item_damage":{"threshold":r.f32()?,"base":r.f32()?,"factor":r.f32()?},
        "bypassed_by":optional(r,holder_set)?,"block_sound":optional(r,sound)?,"disable_sound":optional(r,sound)?}))
}
pub(super) fn piercing(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(
        json!({"knockback":r.bool()?,"dismounts":r.bool()?,"sound":optional(r,sound)?,"hit_sound":optional(r,sound)?}),
    )
}
pub(super) fn kinetic(r: &mut Reader<'_>) -> Result<Value, Error> {
    fn condition(r: &mut Reader<'_>) -> Result<Value, Error> {
        Ok(json!({"max_ticks":r.var()?,"min_speed":r.f32()?,"min_relative_speed":r.f32()?}))
    }
    Ok(
        json!({"contact_cooldown_ticks":r.var()?,"delay_ticks":r.var()?,
        "dismount":optional(r,condition)?,"knockback":optional(r,condition)?,"damage":optional(r,condition)?,
        "forward_movement":r.f32()?,"damage_multiplier":r.f32()?,"sound":optional(r,sound)?,"hit_sound":optional(r,sound)?}),
    )
}
pub(super) fn predicates(r: &mut Reader<'_>, depth: usize) -> Result<Value, Error> {
    check_depth(depth)?;
    list(r, 5, |r| {
        Ok(json!({"blocks":optional(r,holder_set)?,
        "properties":optional(r,|r|list(r,3,|r| {
            let name=text(r)?;
            if r.bool()? {Ok(json!({"name":name,"value":text(r)?}))}
            else {Ok(json!({"name":name,"min":text(r)?,"max":text(r)?}))}
        }))?,"nbt":optional(r,compound)?,"components":matchers(r,depth)?}))
    })
}
fn matchers(r: &mut Reader<'_>, depth: usize) -> Result<Value, Error> {
    let n = r.count(target::ITEM_COMPONENTS.len())?;
    let mut exact = BTreeMap::new();
    for _ in 0..n {
        let spec = component(r)?;
        if exact.insert(spec.name, read(spec, r, depth)?).is_some() {
            return Err(Error::Invalid("duplicate exact component matcher"));
        }
    }
    let partial = list_limit(r, 4, 64, |r| {
        Ok(json!({"predicate":r.bool()?,"id":nonnegative(r)?,"data":compound(r)?}))
    })?;
    Ok(json!({"exact":exact,"partial":partial}))
}
