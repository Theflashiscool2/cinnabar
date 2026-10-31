//! Bounded shared wire shapes; resource keys and profile URLs remain inert data.
use super::*;

pub(super) fn check_depth(depth: usize) -> Result<(), Error> {
    if depth > 64 {
        Err(Error::Invalid("item component nesting budget"))
    } else {
        Ok(())
    }
}
pub(super) fn text(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(json!(r.string(32767)?))
}
pub(super) fn key(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(json!(super::super::super::dynamics::identifier(
        r.string(32767)?
    )?))
}
pub(super) fn component_text(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(serde_json::to_value(nbt::read(r)?)?)
}
pub(super) fn compound(r: &mut Reader<'_>) -> Result<Value, Error> {
    let value = nbt::read(r)?;
    if !matches!(value, nbt::Value::Compound(_)) {
        return Err(Error::Invalid("item expected compound NBT"));
    }
    Ok(serde_json::to_value(value)?)
}
pub(super) fn optional(
    r: &mut Reader<'_>,
    read: impl FnOnce(&mut Reader<'_>) -> Result<Value, Error>,
) -> Result<Value, Error> {
    if r.bool()? { read(r) } else { Ok(Value::Null) }
}
pub(super) fn list(
    r: &mut Reader<'_>,
    min: usize,
    read: impl FnMut(&mut Reader<'_>) -> Result<Value, Error>,
) -> Result<Value, Error> {
    list_limit(r, min, usize::MAX, read)
}
pub(super) fn list_limit(
    r: &mut Reader<'_>,
    min: usize,
    max: usize,
    mut read: impl FnMut(&mut Reader<'_>) -> Result<Value, Error>,
) -> Result<Value, Error> {
    // Limit both wire bytes and expanded list storage, as for the outer inventory snapshot.
    let n = r.count(
        (r.0.len() / min)
            .min(max)
            .min(crate::stream::MAX_UNCOMPRESSED / size_of::<Value>()),
    )?;
    let mut values = Vec::with_capacity(n);
    let mut retained = n.saturating_mul(size_of::<Value>());
    for _ in 0..n {
        let value = read(r)?;
        retained = retained.saturating_add(retained_bytes(&value)?);
        if retained > crate::stream::MAX_UNCOMPRESSED {
            return Err(Error::Invalid("expanded item list budget"));
        }
        values.push(value);
    }
    Ok(Value::Array(values))
}
pub(super) fn holder(
    r: &mut Reader<'_>,
    read: impl FnOnce(&mut Reader<'_>) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let id = nonnegative(r)?;
    if id == 0 {
        Ok(json!({"inline":read(r)?}))
    } else {
        Ok(json!({"id":id-1}))
    }
}
pub(super) fn sound(r: &mut Reader<'_>) -> Result<Value, Error> {
    holder(r, |r| {
        Ok(json!({"name":text(r)?,"fixed_range":optional(r,|r| Ok(json!(r.f32()?)))?}))
    })
}
pub(super) fn resolvable(r: &mut Reader<'_>, float: bool) -> Result<Value, Error> {
    if r.bool()? {
        Ok(json!({"constant":if float {json!(r.f32()?)} else {json!(r.i32()?)}}))
    } else {
        Ok(json!({"key":key(r)?}))
    }
}
pub(super) fn typed_entity(r: &mut Reader<'_>) -> Result<Value, Error> {
    Ok(json!({"type":nonnegative(r)?,"data":compound(r)?}))
}
pub(super) fn template(r: &mut Reader<'_>, depth: usize) -> Result<Value, Error> {
    check_depth(depth)?;
    // Templates are item ID then count, unlike the outer optional stack's count then ID.
    let item = nonnegative(r)? as u32;
    let count = nonnegative(r)? as u32;
    Ok(serde_json::to_value(super::super::stack_components(
        r, item, count, depth,
    )?)?)
}
pub(super) fn filterable(
    r: &mut Reader<'_>,
    read: fn(&mut Reader<'_>) -> Result<Value, Error>,
) -> Result<Value, Error> {
    Ok(json!({"raw":read(r)?,"filtered":optional(r,read)?}))
}
