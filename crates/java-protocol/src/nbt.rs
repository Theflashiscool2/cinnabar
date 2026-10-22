//! Bounded, unnamed network NBT. Registry values retain their wire types and order.
use crate::{ProtocolError as Error, wire::Reader};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Value {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Bytes(Vec<u8>),
    String(String),
    List(Vec<Value>),
    Compound(BTreeMap<String, Value>),
    Ints(Vec<i32>),
    Longs(Vec<i64>),
}
/// Admission budgets are explicit and report errors; they never truncate registry facts.
pub fn read(reader: &mut Reader<'_>) -> Result<Value, Error> {
    let tag = reader.byte()?;
    let mut remaining = 262_144;
    payload(reader, tag, 0, &mut remaining)
}
fn text(reader: &mut Reader<'_>) -> Result<String, Error> {
    let n = u16::from_be_bytes(reader.take(2)?.try_into().unwrap()) as usize;
    cesu8::from_java_cesu8(reader.take(n)?)
        .map(|s| s.into_owned())
        .map_err(|_| Error::Invalid("NBT modified UTF-8"))
}
fn count(reader: &mut Reader<'_>, width: usize, remaining: usize) -> Result<usize, Error> {
    let n = usize::try_from(reader.i32()?).map_err(|_| Error::Invalid("NBT negative array"))?;
    if n > remaining || n > reader.0.len() / width {
        return Err(Error::Invalid("NBT array budget"));
    }
    Ok(n)
}
fn payload(
    r: &mut Reader<'_>,
    tag: u8,
    depth: usize,
    remaining: &mut usize,
) -> Result<Value, Error> {
    if depth > 64 || *remaining == 0 {
        return Err(Error::Invalid("NBT nesting/node budget"));
    }
    *remaining -= 1;
    Ok(match tag {
        1 => Value::Byte(r.byte()? as i8),
        2 => Value::Short(i16::from_be_bytes(r.take(2)?.try_into().unwrap())),
        3 => Value::Int(r.i32()?),
        4 => Value::Long(r.i64()?),
        5 => Value::Float(r.f32()?),
        6 => Value::Double(r.f64()?),
        7 => {
            let n = count(r, 1, *remaining)?;
            *remaining -= n;
            Value::Bytes(r.take(n)?.to_vec())
        }
        8 => Value::String(text(r)?),
        9 => {
            let kind = r.byte()?;
            let n = count(r, 1, *remaining)?;
            if kind == 0 && n != 0 {
                return Err(Error::Invalid("NBT nonempty end list"));
            }
            let mut values = Vec::with_capacity(n);
            for _ in 0..n {
                values.push(payload(r, kind, depth + 1, remaining)?);
            }
            Value::List(values)
        }
        10 => {
            let mut values = BTreeMap::new();
            loop {
                let kind = r.byte()?;
                if kind == 0 {
                    break;
                }
                let key = text(r)?;
                let value = payload(r, kind, depth + 1, remaining)?;
                if values.insert(key, value).is_some() {
                    return Err(Error::Invalid("duplicate NBT name"));
                }
            }
            Value::Compound(values)
        }
        11 => {
            let n = count(r, 4, *remaining)?;
            *remaining -= n;
            Value::Ints((0..n).map(|_| r.i32()).collect::<Result<_, _>>()?)
        }
        12 => {
            let n = count(r, 8, *remaining)?;
            *remaining -= n;
            Value::Longs((0..n).map(|_| r.i64()).collect::<Result<_, _>>()?)
        }
        _ => return Err(Error::Invalid("NBT tag")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_unnamed_compound_and_modified_utf8() {
        let bytes = [10, 3, 0, 5, b'h', b'e', b'i', b'g', b'h', 0, 0, 1, 0, 0];
        let mut r = Reader(&bytes);
        let Value::Compound(value) = read(&mut r).unwrap() else {
            panic!("compound")
        };
        assert_eq!(value["heigh"], Value::Int(256));
        r.finish().unwrap();
        let mut r = Reader(&[8, 0, 2, 0xc0, 0x80]);
        assert_eq!(read(&mut r).unwrap(), Value::String("\0".into()));
    }
    #[test]
    fn malformed_lengths_and_nesting_are_bounded() {
        for bytes in [
            &[7, 255, 255, 255, 255][..],
            &[9, 10, 127, 255, 255, 255],
            &[13],
            &[10, 3],
        ] {
            assert!(read(&mut Reader(bytes)).is_err());
        }
    }
}
