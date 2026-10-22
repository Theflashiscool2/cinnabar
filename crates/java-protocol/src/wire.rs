//! Checked field reads; every fixed packet must consume its complete payload.
use crate::{ProtocolError as Error, codec};

pub struct Reader<'a>(pub &'a [u8]);
impl<'a> Reader<'a> {
    pub fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let (value, rest) = self.0.split_at_checked(count).ok_or(Error::Truncated)?;
        self.0 = rest;
        Ok(value)
    }
    pub fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self) -> Result<bool, Error> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Invalid("boolean")),
        }
    }
    pub fn var(&mut self) -> Result<i32, Error> {
        codec::read_varint(&mut self.0)
    }
    pub fn count(&mut self, limit: usize) -> Result<usize, Error> {
        let n = usize::try_from(self.var()?).map_err(|_| Error::Invalid("negative count"))?;
        if n > limit || n > self.0.len() {
            return Err(Error::Invalid("count exceeds input or budget"));
        }
        Ok(n)
    }
    pub fn string(&mut self, max: usize) -> Result<String, Error> {
        Ok(codec::read_string(&mut self.0, max)?.to_owned())
    }
    pub fn bytes(&mut self, max: usize) -> Result<Vec<u8>, Error> {
        let n = self.count(max)?;
        Ok(self.take(n)?.to_vec())
    }
    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i64(&mut self) -> Result<i64, Error> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32, Error> {
        let n = f32::from_bits(self.i32()? as u32);
        if !n.is_finite() {
            return Err(Error::Invalid("non-finite float"));
        }
        Ok(n)
    }
    pub fn f64(&mut self) -> Result<f64, Error> {
        let n = f64::from_bits(self.i64()? as u64);
        if !n.is_finite() {
            return Err(Error::Invalid("non-finite double"));
        }
        Ok(n)
    }
    pub fn uuid(&mut self) -> Result<uuid::Uuid, Error> {
        Ok(uuid::Uuid::from_bytes(self.take(16)?.try_into().unwrap()))
    }
    pub fn finish(self) -> Result<(), Error> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Error::TrailingData)
        }
    }
}
/// Encodes one bounded byte array.
pub fn bytes(value: &[u8], out: &mut Vec<u8>) -> Result<(), Error> {
    codec::write_varint(
        i32::try_from(value.len()).map_err(|_| Error::InvalidFrameLength)?,
        out,
    );
    out.extend_from_slice(value);
    Ok(())
}
/// Encodes a protocol string at the standard 32767 UTF-16-unit bound.
pub fn string(value: &str, out: &mut Vec<u8>) -> Result<(), Error> {
    codec::write_string(value, 32767, out)
}
