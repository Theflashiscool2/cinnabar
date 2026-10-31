//! Item use metadata. These values do not themselves consume items or apply effects.
use super::*;

pub(super) fn consumable(r: &mut Reader<'_>, depth: usize) -> Result<Value, Error> {
    Ok(
        json!({"seconds":r.f32()?,"animation":nonnegative(r)?,"sound":sound(r)?,
        "particles":r.bool()?,"effects":list(r,1,|r|consume_effect(r,depth))?}),
    )
}
pub(super) fn consume_effect(r: &mut Reader<'_>, depth: usize) -> Result<Value, Error> {
    Ok(match nonnegative(r)? {
        0 => json!({"kind":"apply","effects":list(r,7,|r|effect(r,depth))?,"probability":r.f32()?}),
        1 => json!({"kind":"remove","effects":holder_set(r)?}),
        2 => json!({"kind":"clear"}),
        3 => json!({"kind":"teleport","diameter":r.f32()?,"directional_particles":r.bool()?}),
        4 => json!({"kind":"sound","sound":sound(r)?}),
        _ => return Err(Error::Invalid("unknown item consume effect")),
    })
}
fn effect(r: &mut Reader<'_>, depth: usize) -> Result<Value, Error> {
    Ok(json!({"id":nonnegative(r)?,"details":details(r,depth)?}))
}
fn details(r: &mut Reader<'_>, depth: usize) -> Result<Value, Error> {
    check_depth(depth)?;
    Ok(
        json!({"amplifier":r.var()?,"duration":r.var()?,"ambient":r.bool()?,"particles":r.bool()?,
        "icon":r.bool()?,"hidden":optional(r,|r|details(r,depth+1))?}),
    )
}
pub(super) fn potion(r: &mut Reader<'_>, depth: usize) -> Result<Value, Error> {
    Ok(json!({"potion":optional(r,|r|Ok(json!(nonnegative(r)?)))?,
        "color":optional(r,|r|Ok(json!(r.i32()?)))?,"effects":list(r,7,|r|effect(r,depth))?,
        "name":optional(r,text)?}))
}
