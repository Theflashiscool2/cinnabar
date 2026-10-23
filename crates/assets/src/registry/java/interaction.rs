//! Native outline/interaction shapes, kept separate from physical collision and visual models.
use super::physics::Bounds;
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub enum Shape {
    Static {
        outline: Box<[Bounds]>,
        interaction: Box<[Bounds]>,
    },
    /// Needs position/entity context. An absent implementation must never become empty space.
    Contextual,
}
#[derive(Debug)]
pub struct State {
    pub shape: Shape,
    pub destroy_speed: f32,
    pub requires_tool: bool,
}
#[derive(Debug)]
pub struct Table(Box<[State]>);
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Java interaction report does not match the pinned SHA-256")]
    Hash,
    #[error("invalid Java interaction report: {0}")]
    Invalid(&'static str),
}
impl Table {
    pub fn load(
        bytes: &[u8],
        expected_sha256: &str,
        expected_states: usize,
    ) -> Result<Self, Error> {
        if format!("{:x}", Sha256::digest(bytes)) != expected_sha256 {
            return Err(Error::Hash);
        }
        let mut reader = Reader(bytes);
        if reader.take::<8>()? != *b"CBJHITS1" {
            return Err(Error::Invalid("schema"));
        }
        let count = reader.u32()? as usize;
        if count == 0 || count != expected_states || count > reader.0.len() / 18 {
            return Err(Error::Invalid("state count"));
        }
        let mut states = Vec::with_capacity(count);
        for id in 0..count {
            if reader.u32()? as usize != id {
                return Err(Error::Invalid("state order"));
            }
            let contextual = reader.boolean()?;
            let destroy_speed = f32::from_be_bytes(reader.take()?);
            if !destroy_speed.is_finite() {
                return Err(Error::Invalid("destroy speed"));
            }
            let requires_tool = reader.boolean()?;
            let outline = reader.boxes()?;
            let interaction = reader.boxes()?;
            let shape = if contextual {
                if !outline.is_empty() || !interaction.is_empty() {
                    return Err(Error::Invalid("contextual shape has static boxes"));
                }
                Shape::Contextual
            } else {
                Shape::Static {
                    outline,
                    interaction,
                }
            };
            states.push(State {
                shape,
                destroy_speed,
                requires_tool,
            });
        }
        if !reader.0.is_empty() {
            return Err(Error::Invalid("trailing data"));
        }
        Ok(Self(states.into()))
    }
    pub fn state(&self, id: u32) -> Option<&State> {
        self.0.get(id as usize)
    }
}
struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let bytes = self.0.get(..N).ok_or(Error::Invalid("truncated report"))?;
        let result = bytes.try_into().expect("checked length");
        self.0 = &self.0[N..];
        Ok(result)
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_be_bytes(self.take()?))
    }
    fn boolean(&mut self) -> Result<bool, Error> {
        match self.take()? {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err(Error::Invalid("boolean")),
        }
    }
    fn boxes(&mut self) -> Result<Box<[Bounds]>, Error> {
        let count = self.u32()? as usize;
        if count > self.0.len() / 48 {
            return Err(Error::Invalid("shape length"));
        }
        let mut boxes = Vec::with_capacity(count);
        for _ in 0..count {
            let mut values = [0.; 6];
            for value in &mut values {
                *value = f64::from_be_bytes(self.take()?);
            }
            if values.iter().any(|v| !v.is_finite()) || (0..3).any(|i| values[i] >= values[i + 3]) {
                return Err(Error::Invalid("shape bounds"));
            }
            boxes.push(Bounds {
                min: values[..3].try_into().unwrap(),
                max: values[3..].try_into().unwrap(),
            });
        }
        Ok(boxes.into())
    }
}

#[cfg(test)]
mod tests;
