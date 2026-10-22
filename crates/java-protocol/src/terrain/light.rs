//! Protocol-777 light masks use Java BitSet byte arrays (least-significant bit first).
use crate::{ProtocolError as Error, wire::Reader};
#[derive(Clone, Debug, serde::Serialize)]
pub struct Light {
    /// One entry per light section, including one boundary section at each end.
    /// None means unchanged/unknown; Some(empty) means explicitly zero light.
    pub sky: Vec<Option<Vec<u8>>>,
    pub block: Vec<Option<Vec<u8>>>,
}
impl Light {
    pub fn read(r: &mut Reader<'_>, sections: usize) -> Result<Self, Error> {
        let n = sections + 2;
        let sky = mask(r, n)?;
        let block = mask(r, n)?;
        let empty_sky = mask(r, n)?;
        let empty_block = mask(r, n)?;
        Ok(Self {
            sky: channel(r, &sky, &empty_sky)?,
            block: channel(r, &block, &empty_block)?,
        })
    }
}
fn mask(r: &mut Reader<'_>, n: usize) -> Result<Vec<bool>, Error> {
    let bytes = r.bytes(n.div_ceil(8))?;
    if (n..bytes.len() * 8).any(|bit| bytes[bit / 8] & (1 << (bit % 8)) != 0) {
        return Err(Error::Invalid("Java light mask outside dimension"));
    }
    Ok((0..n)
        .map(|bit| {
            bytes
                .get(bit / 8)
                .is_some_and(|b| b & (1 << (bit % 8)) != 0)
        })
        .collect())
}
fn channel(
    r: &mut Reader<'_>,
    mask: &[bool],
    empty: &[bool],
) -> Result<Vec<Option<Vec<u8>>>, Error> {
    let count = r.count(mask.len())?;
    if count != mask.iter().filter(|b| **b).count() || mask.iter().zip(empty).any(|(a, b)| *a && *b)
    {
        return Err(Error::Invalid("Java light mask/update mismatch"));
    }
    mask.iter()
        .zip(empty)
        .map(|(has, empty)| {
            if *has {
                let data = r.bytes(2048)?;
                if data.len() != 2048 {
                    return Err(Error::Invalid("Java light nibble array length"));
                }
                Ok(Some(data))
            } else {
                Ok(empty.then(Vec::new))
            }
        })
        .collect()
}
