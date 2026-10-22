//! Java collision and surface facts exported from the pinned server, never from visual models.
use super::source::Identifier;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Debug)]
pub enum Collision {
    Static(Box<[Bounds]>),
    /// Requires entity, position or block-entity context; no empty-shape fallback is allowed.
    Dynamic,
}

#[derive(Debug)]
pub struct State {
    pub collision: Collision,
    pub special_callbacks: bool,
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    pub bounce_restitution: f32,
    pub is_air: bool,
    pub fluid_empty: bool,
    pub fluid: Identifier,
}

#[derive(Debug)]
pub struct Table(Box<[State]>);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Java physics report does not match the pinned SHA-256")]
    Hash,
    #[error("invalid Java physics report: {0}")]
    Invalid(&'static str),
}

impl Table {
    /// Callers supply the manifest hash and registry count before any report data is admitted.
    pub fn load(
        bytes: &[u8],
        expected_sha256: &str,
        expected_states: usize,
    ) -> Result<Self, Error> {
        if format!("{:x}", Sha256::digest(bytes)) != expected_sha256 {
            return Err(Error::Hash);
        }
        let mut r = Reader(bytes);
        if r.take::<8>()? != *b"CBJPHYS1" {
            return Err(Error::Invalid("schema"));
        }
        let count = r.u32()? as usize;
        if count == 0 || count != expected_states || count > r.0.len() / 28 {
            return Err(Error::Invalid("state count"));
        }
        let mut states = Vec::with_capacity(count);
        for id in 0..count {
            if r.u32()? as usize != id {
                return Err(Error::Invalid("state ID order"));
            }
            let [flags] = r.take()?;
            if flags & !3 != 0 {
                return Err(Error::Invalid("unknown behavior flags"));
            }
            let [friction, speed_factor, jump_factor, bounce_restitution] =
                [r.factor()?, r.factor()?, r.factor()?, r.factor()?];
            let is_air = r.boolean()?;
            let fluid_empty = r.boolean()?;
            let length = u16::from_be_bytes(r.take()?) as usize;
            let name = r.0.get(..length).ok_or(Error::Invalid("truncated fluid"))?;
            let fluid = Identifier::parse(
                std::str::from_utf8(name).map_err(|_| Error::Invalid("fluid UTF-8"))?,
            )
            .map_err(|_| Error::Invalid("fluid identifier"))?;
            r.0 = &r.0[length..];
            let boxes = r.u32()? as usize;
            if boxes > r.0.len() / 48 || (flags & 1 != 0 && boxes != 0) {
                return Err(Error::Invalid("collision shape length"));
            }
            let mut bounds = Vec::with_capacity(boxes);
            for _ in 0..boxes {
                let min = [r.f64()?, r.f64()?, r.f64()?];
                let max = [r.f64()?, r.f64()?, r.f64()?];
                if (0..3).any(|i| !min[i].is_finite() || !max[i].is_finite() || min[i] >= max[i]) {
                    return Err(Error::Invalid("collision bounds"));
                }
                bounds.push(Bounds { min, max });
            }
            states.push(State {
                collision: if flags & 1 != 0 {
                    Collision::Dynamic
                } else {
                    Collision::Static(bounds.into())
                },
                special_callbacks: flags & 2 != 0,
                friction,
                speed_factor,
                jump_factor,
                bounce_restitution,
                is_air,
                fluid_empty,
                fluid,
            });
        }
        if !r.0.is_empty() {
            return Err(Error::Invalid("trailing bytes"));
        }
        Ok(Self(states.into()))
    }
    pub fn state(&self, id: u32) -> Option<&State> {
        self.0.get(id as usize)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn boolean(&mut self) -> Result<bool, Error> {
        match self.take()? {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err(Error::Invalid("boolean")),
        }
    }
    fn take<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let bytes = self.0.get(..N).ok_or(Error::Invalid("truncated field"))?;
        let value = bytes.try_into().expect("checked field length");
        self.0 = &self.0[N..];
        Ok(value)
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_be_bytes(self.take()?))
    }
    fn f64(&mut self) -> Result<f64, Error> {
        Ok(f64::from_be_bytes(self.take()?))
    }
    fn factor(&mut self) -> Result<f32, Error> {
        let n = f32::from_be_bytes(self.take()?);
        if !n.is_finite() || n < 0. {
            return Err(Error::Invalid("surface factor"));
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(flags: u8, shape: &[u8]) -> Vec<u8> {
        // Independent wire fixture: one state, surface factors .5/1/1/0, non-air,
        // fixture:none fluid. Coordinates below are IEEE-754, not loader-generated data.
        let mut bytes = b"CBJPHYS1\0\0\0\x01\0\0\0\0".to_vec();
        bytes.push(flags);
        bytes.extend([
            0x3f, 0, 0, 0, 0x3f, 0x80, 0, 0, 0x3f, 0x80, 0, 0, 0, 0, 0, 0, 0,
        ]);
        bytes.extend(b"\x01\0\x0cfixture:none");
        bytes.extend(shape);
        bytes
    }
    fn load(bytes: &[u8]) -> Result<Table, Error> {
        Table::load(bytes, &format!("{:x}", Sha256::digest(bytes)), 1)
    }
    #[test]
    fn static_empty_and_dynamic_are_distinct_and_surface_values_are_native() {
        let empty = fixture(0, &[0; 4]);
        let table = load(&empty).unwrap();
        assert!(
            matches!(&table.state(0).unwrap().collision, Collision::Static(boxes) if boxes.is_empty())
        );
        assert_eq!(table.state(0).unwrap().friction, 0.5);
        assert!(!table.state(0).unwrap().is_air);
        assert!(table.state(1).is_none());
        let dynamic = load(&fixture(3, &[0; 4])).unwrap();
        assert!(matches!(
            dynamic.state(0).unwrap().collision,
            Collision::Dynamic
        ));
        assert!(dynamic.state(0).unwrap().special_callbacks);
        assert!(matches!(
            Table::load(&empty, "wrong hash", 1),
            Err(Error::Hash)
        ));
    }
    #[test]
    fn known_box_truncation_overflow_and_malformed_values() {
        let mut shape = vec![0, 0, 0, 1];
        shape.extend([0; 24]);
        shape.extend([0x3f, 0xf0, 0, 0, 0, 0, 0, 0].repeat(3));
        let bytes = fixture(0, &shape);
        let table = load(&bytes).unwrap();
        let Collision::Static(boxes) = &table.state(0).unwrap().collision else {
            panic!("static shape expected")
        };
        assert_eq!(
            boxes.as_ref(),
            &[Bounds {
                min: [0.; 3],
                max: [1.; 3]
            }]
        );
        for end in 0..bytes.len() {
            assert!(load(&bytes[..end]).is_err(), "prefix {end}");
        }
        assert!(load(&fixture(1, &shape)).is_err());
        assert!(load(&fixture(4, &shape)).is_err());
        assert!(load(&fixture(0, &[255; 4])).is_err());
        let mut malformed = bytes.clone();
        malformed[17..21].copy_from_slice(&f32::NAN.to_be_bytes());
        assert!(load(&malformed).is_err());
        let mut trailing = bytes;
        trailing.push(0);
        assert!(load(&trailing).is_err());
    }
}
