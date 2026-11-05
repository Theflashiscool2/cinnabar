//! Version26.3 player/disguised chat framing and server-owned chat-type decoration.
use super::{
    System,
    window::{Cache, SIGNATURE_BYTES, Signature, WINDOW},
};
use crate::{ProtocolError as Error, nbt::Value, wire::Reader};
use std::collections::BTreeMap;
#[derive(Debug)]
pub(super) struct Player {
    pub global: i32,
    pub signature: Option<Signature>,
    pub last_seen: Vec<Signature>,
    pub content: Option<Value>,
    pub bound: Bound,
}
#[derive(Debug)]
pub(super) struct Bound {
    decoration: Value,
    sender: Value,
    target: Value,
}
fn string(r: &mut Reader<'_>, max: usize) -> Result<String, Error> {
    crate::codec::read_string(&mut r.0, max).map(str::to_owned)
}
pub(super) fn signature(r: &mut Reader<'_>, cache: &Cache) -> Result<Signature, Error> {
    match r.var()? {
        0 => Ok(r.take(SIGNATURE_BYTES)?.try_into().unwrap()),
        n => cache.get(
            n.checked_sub(1)
                .ok_or(Error::Invalid("chat signature index"))?,
        ),
    }
}
pub(super) fn player(
    r: &mut Reader<'_>,
    cache: &Cache,
    registry: &[super::super::facts::RegistryEntry],
) -> Result<Player, Error> {
    let global = r.var()?;
    let _sender = r.uuid()?;
    let index = r.var()?;
    if global < 0 || index < 0 {
        return Err(Error::Invalid("negative Java player chat index"));
    }
    let message_signature = if r.bool()? {
        Some(r.take(SIGNATURE_BYTES)?.try_into().unwrap())
    } else {
        None
    };
    let text = string(r, super::PLAYER_MAX_UTF16)?;
    let _timestamp = r.i64()?;
    let _salt = r.i64()?;
    let count = r.var()?;
    if !(0..=WINDOW as i32).contains(&count) {
        return Err(Error::Invalid("Java last-seen list length"));
    }
    let last_seen = (0..count)
        .map(|_| signature(r, cache))
        .collect::<Result<Vec<_>, _>>()?;
    let unsigned = if r.bool()? {
        Some(crate::nbt::read(r)?)
    } else {
        None
    };
    let content = match r.var()? {
        0 => Some(unsigned.unwrap_or(Value::String(text))),
        1 => None,
        2 => {
            // BitSet is a big-endian long array; bits address Java UTF-16 code units.
            let count = r.var()?;
            if count < 0 || count as usize > r.0.len() / 8 {
                return Err(Error::Invalid("chat filter bitset"));
            }
            let mask = (0..count)
                .map(|_| r.i64().map(|n| n as u64))
                .collect::<Result<Vec<_>, _>>()?;
            let units: Vec<_> = text
                .encode_utf16()
                .enumerate()
                .map(|(i, c)| {
                    if mask
                        .get(i / 64)
                        .is_some_and(|bits| bits & (1 << (i % 64)) != 0)
                    {
                        b'#' as u16
                    } else {
                        c
                    }
                })
                .collect();
            Some(Value::String(String::from_utf16_lossy(&units)))
        }
        _ => return Err(Error::Invalid("chat filter type")),
    };
    Ok(Player {
        global,
        signature: message_signature,
        last_seen,
        content,
        bound: bound(r, registry)?,
    })
}
fn direct_decoration(r: &mut Reader<'_>) -> Result<Value, Error> {
    let key = string(r, 32767)?;
    let count = r.var()?;
    if count < 0 || count as usize > r.0.len() {
        return Err(Error::Invalid("chat decoration parameter length"));
    }
    let parameters = (0..count)
        .map(|_| {
            r.var().map(|n| {
                Value::String(
                    match n {
                        1 => "target",
                        2 => "content",
                        _ => "sender",
                    }
                    .into(),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let style = crate::nbt::read(r)?;
    Ok(Value::Compound(BTreeMap::from([
        ("translation_key".into(), Value::String(key)),
        ("parameters".into(), Value::List(parameters)),
        ("style".into(), style),
    ])))
}
fn bound(
    r: &mut Reader<'_>,
    registry: &[super::super::facts::RegistryEntry],
) -> Result<Bound, Error> {
    let id = r.var()?;
    let decoration = if id == 0 {
        let chat = direct_decoration(r)?;
        let _narration = direct_decoration(r)?;
        chat
    } else {
        let value = id
            .checked_sub(1)
            .and_then(|id| usize::try_from(id).ok())
            .and_then(|i| registry.get(i))
            .ok_or(Error::Invalid("unresolved Java chat type"))?;
        let Value::Compound(fields) = &value.value else {
            return Err(Error::Invalid("Java chat type compound"));
        };
        fields
            .get("chat")
            .cloned()
            .ok_or(Error::Invalid("Java chat decoration"))?
    };
    let sender = crate::nbt::read(r)?;
    let target = if r.bool()? {
        crate::nbt::read(r)?
    } else {
        Value::String(String::new())
    };
    Ok(Bound {
        decoration,
        sender,
        target,
    })
}
impl Bound {
    pub fn decorate(self, content: Value) -> Result<System, Error> {
        let Value::Compound(mut fields) = self.decoration else {
            return Err(Error::Invalid("chat decoration compound"));
        };
        let Some(Value::String(key)) = fields.remove("translation_key") else {
            return Err(Error::Invalid("chat decoration translation"));
        };
        let Some(Value::List(parameters)) = fields.remove("parameters") else {
            return Err(Error::Invalid("chat decoration parameters"));
        };
        let mut args = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            args.push(match parameter {
                Value::String(s) if s == "sender" => self.sender.clone(),
                Value::String(s) if s == "target" => self.target.clone(),
                Value::String(s) if s == "content" => content.clone(),
                _ => return Err(Error::Invalid("chat decoration parameter")),
            });
        }
        let mut style = match fields.remove("style") {
            Some(Value::Compound(style)) => style,
            None => BTreeMap::new(),
            _ => return Err(Error::Invalid("chat decoration style")),
        };
        style.insert("translate".into(), Value::String(key));
        style.insert("with".into(), Value::List(args));
        Ok(System {
            content: Value::Compound(style),
            overlay: false,
        })
    }
}
pub(super) fn disguised(
    r: &mut Reader<'_>,
    registry: &[super::super::facts::RegistryEntry],
) -> Result<System, Error> {
    let content = crate::nbt::read(r)?;
    bound(r, registry)?.decorate(content)
}
