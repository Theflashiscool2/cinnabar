//! Wire-preserved item presentation metadata, without fetching or executing its contents.
use super::*;

pub(super) fn custom_model(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(json!({"floats":list(r,4,|r|Ok(json!(r.f32()?)))?,
        "flags":list(r,1,|r|Ok(json!(r.bool()?)))?,
        "strings":list(r,1,text)?,"colors":list(r,4,|r|Ok(json!(r.i32()?)))?}))
}
pub(super) fn profile(r: &mut Reader<'_>) -> Result<Value, Error> {
    let static_profile = r.bool()?;
    let uuid = |r: &mut Reader<'_>| Ok(json!(r.uuid()?.to_string()));
    let (id, name) = if static_profile {
        (uuid(r)?, text(r)?)
    } else {
        let name = optional(r, text)?;
        (optional(r, uuid)?, name)
    };
    let properties = list(r, 3, |r| {
        Ok(json!({"name":text(r)?,"value":text(r)?,"signature":optional(r,text)?}))
    })?;
    Ok(
        json!({"dynamic":!static_profile,"id":id,"name":name,"properties":properties,
        "body":optional(r,key)?,"cape":optional(r,key)?,"elytra":optional(r,key)?,
        "slim":optional(r,|r|Ok(json!(r.bool()?)))?}),
    )
}
pub(super) fn book(r: &mut Reader<'_>, written: bool) -> Result<Value, Error> {
    if !written {
        return Ok(json!({"pages":list(r,2,|r|filterable(r,text))?}));
    }
    Ok(
        json!({"title":filterable(r,text)?,"author":text(r)?,"generation":r.var()?,
        "pages":list(r,2,|r|filterable(r,component_text))?,"resolved":r.bool()?}),
    )
}
pub(super) fn sign(r: &mut Reader<'_>) -> Result<Value, Error> {
    fn lines(r: &mut Reader<'_>) -> Result<Value, Error> {
        let mut lines = Vec::with_capacity(4);
        for _ in 0..4 {
            lines.push(component_text(r)?);
        }
        Ok(json!(lines))
    }
    Ok(
        json!({"messages":lines(r)?,"filtered_messages":optional(r,lines)?,"color":nonnegative(r)?,"glowing":r.bool()?}),
    )
}
pub(super) fn explosion(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(
        json!({"shape":nonnegative(r)?,"colors":list(r,4,|r|Ok(json!(r.i32()?)))?,
        "fade_colors":list(r,4,|r|Ok(json!(r.i32()?)))?,"trail":r.bool()?,"twinkle":r.bool()?}),
    )
}
pub(super) fn properties(r: &mut Reader<'_>) -> Result<Value, Error> {
    let n = r.count(r.0.len() / 2)?;
    let mut values = BTreeMap::new();
    for _ in 0..n {
        values.insert(r.string(32767)?, r.string(32767)?);
    }
    Ok(json!(values))
}
pub(super) fn painting(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(
        json!({"width":nonnegative(r)?,"height":nonnegative(r)?,"asset":key(r)?,
        "title":optional(r,component_text)?,"author":optional(r,component_text)?}),
    )
}
pub(super) fn trim_material(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(json!({"palette":key(r)?,"description":component_text(r)?}))
}
pub(super) fn trim_pattern(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(json!({"asset":key(r)?,"description":component_text(r)?,"decal":r.bool()?}))
}
pub(super) fn instrument(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(
        json!({"sound":sound(r)?,"use_duration":r.f32()?,"range":r.f32()?,
        "durability_damage":r.var()?,"description":component_text(r)?}),
    )
}
pub(super) fn song(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(
        json!({"sound":sound(r)?,"description":component_text(r)?,"length_seconds":r.f32()?,"comparator_output":r.var()?}),
    )
}
