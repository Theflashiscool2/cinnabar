//! Pinned Java terrain wire schemas, independent of Bedrock chunk storage.
mod light;
mod palette;
#[cfg(test)]
mod tests;
use crate::{ProtocolError as Error, nbt, wire::Reader};
pub use light::Light;
pub use palette::Palette;
use palette::nonnegative;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Section {
    pub block_count: u16,
    pub fluid_count: u16,
    pub blocks: Palette,
    pub biomes: Palette,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct BlockEntity {
    pub x: u8,
    pub y: i16,
    pub z: u8,
    pub kind: u32,
    pub data: Option<nbt::Value>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    pub heightmaps: Vec<(u32, Vec<u64>)>,
    pub sections: Vec<Section>,
    pub block_entities: Vec<BlockEntity>,
    pub light: Light,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct BlockUpdate {
    pub position: [i32; 3],
    pub state: u32,
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum Packet {
    Chunk(Chunk),
    Light {
        x: i32,
        z: i32,
        light: Light,
    },
    Blocks(Vec<BlockUpdate>),
    BlockEntity {
        position: [i32; 3],
        kind: u32,
        data: Option<nbt::Value>,
    },
    BlockEvent {
        position: [i32; 3],
        action: u8,
        value: u8,
        block: u32,
    },
    Unload {
        x: i32,
        z: i32,
    },
    Center {
        x: i32,
        z: i32,
    },
    Radius(u32),
    Biomes(Vec<(i32, i32, Vec<Palette>)>),
}
/// Dimension geometry needed to decode sections; obtained from configuration registries.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub min_y: i32,
    pub height: i32,
    pub biomes: usize,
}
impl Context {
    pub fn sections(self) -> usize {
        (self.height / 16) as usize
    }
    pub fn new(min_y: i32, height: i32, biomes: usize) -> Result<Self, Error> {
        // Java DimensionType's codec permits [-2032,2031] within a 4064-block height.
        if min_y % 16 != 0
            || height % 16 != 0
            || height <= 0
            || height > 4064
            || min_y < -2032
            || min_y.checked_add(height).is_none_or(|end| end > 2032)
            || biomes == 0
        {
            return Err(Error::Invalid("Java dimension geometry/biome registry"));
        }
        Ok(Self {
            min_y,
            height,
            biomes,
        })
    }
    pub fn decode(self, name: &str, r: &mut Reader<'_>) -> Result<Option<Packet>, Error> {
        Ok(Some(match name {
            "ClientboundLevelChunkWithLightPacket" => Packet::Chunk(self.chunk(r)?),
            "ClientboundLightUpdatePacket" => Packet::Light {
                x: r.var()?,
                z: r.var()?,
                light: Light::read(r, self.sections())?,
            },
            "ClientboundForgetLevelChunkPacket" => {
                let p = r.i64()?;
                Packet::Unload {
                    x: p as i32,
                    z: (p >> 32) as i32,
                }
            }
            "ClientboundBlockEventPacket" => Packet::BlockEvent {
                position: block_position(r.i64()?),
                action: r.byte()?,
                value: r.byte()?,
                block: nonnegative(r)?,
            },
            "ClientboundBlockEntityDataPacket" => {
                let position = block_position(r.i64()?);
                let kind = nonnegative(r)?;
                let data = if r.0.first() == Some(&0) {
                    r.byte()?;
                    None
                } else {
                    match nbt::read(r)? {
                        v @ nbt::Value::Compound(_) => Some(v),
                        _ => return Err(Error::Invalid("Java block entity compound")),
                    }
                };
                Packet::BlockEntity {
                    position,
                    kind,
                    data,
                }
            }
            "ClientboundBlockUpdatePacket" => Packet::Blocks(vec![BlockUpdate {
                position: block_position(r.i64()?),
                state: nonnegative(r)?,
            }]),
            "ClientboundSectionBlocksUpdatePacket" => Packet::Blocks(section_updates(r)?),
            "ClientboundSetChunkCacheCenterPacket" => Packet::Center {
                x: r.var()?,
                z: r.var()?,
            },
            "ClientboundSetChunkCacheRadiusPacket" => Packet::Radius(nonnegative(r)?),
            "ClientboundChunksBiomesPacket" => {
                let n = r.count(r.0.len() / 9)?;
                let mut chunks = Vec::with_capacity(n);
                for _ in 0..n {
                    let p = r.i64()?;
                    let size = r.count(r.0.len())?;
                    let mut data = Reader(r.take(size)?);
                    let mut palettes = Vec::with_capacity(self.sections());
                    for _ in 0..self.sections() {
                        palettes.push(self.biome(&mut data)?);
                    }
                    data.finish()?;
                    chunks.push((p as i32, (p >> 32) as i32, palettes));
                }
                Packet::Biomes(chunks)
            }
            _ => return Ok(None),
        }))
    }
    fn biome(self, r: &mut Reader<'_>) -> Result<Palette, Error> {
        let palette = Palette::read(r, 64, 3)?;
        palette.validate_registry(self.biomes)?;
        Ok(palette)
    }
    fn chunk(self, r: &mut Reader<'_>) -> Result<Chunk, Error> {
        let x = r.i32()?;
        let z = r.i32()?;
        let n = r.count(6)?;
        let mut heightmaps = Vec::with_capacity(n);
        let bits = 32 - (self.height as u32).leading_zeros();
        let words = 256usize.div_ceil(64 / bits as usize);
        for _ in 0..n {
            let kind = nonnegative(r)?;
            if kind > 5 || heightmaps.iter().any(|(k, _)| *k == kind) {
                return Err(Error::Invalid("Java heightmap kind"));
            }
            if r.count(words)? != words {
                return Err(Error::Invalid("Java heightmap word count"));
            }
            let data = r
                .take(words * 8)?
                .as_chunks::<8>()
                .0
                .iter()
                .map(|b| u64::from_be_bytes(*b))
                .collect();
            let data: Vec<u64> = data;
            for index in 0..256 {
                let per_word = 64 / bits as usize;
                let value = (data[index / per_word] >> (index % per_word * bits as usize))
                    & ((1u64 << bits) - 1);
                if value > self.height as u64 {
                    return Err(Error::Invalid("Java heightmap value outside dimension"));
                }
            }
            heightmaps.push((kind, data));
        }
        let size = r.count(r.0.len())?;
        let mut data = Reader(r.take(size)?);
        let mut sections = Vec::with_capacity(self.sections());
        for _ in 0..self.sections() {
            let block_count = u16::from_be_bytes(data.take(2)?.try_into().unwrap());
            let fluid_count = u16::from_be_bytes(data.take(2)?.try_into().unwrap());
            if block_count > 4096 || fluid_count > 4096 {
                return Err(Error::Invalid("Java section block/fluid count"));
            }
            sections.push(Section {
                block_count,
                fluid_count,
                blocks: {
                    let palette = Palette::read(&mut data, 4096, 8)?;
                    palette.validate_registry(crate::target::BLOCK_STATE_COUNT)?;
                    palette
                },
                biomes: self.biome(&mut data)?,
            });
        }
        data.finish()?;
        let n = r.count(r.0.len() / 5)?;
        let mut block_entities = Vec::with_capacity(n);
        for _ in 0..n {
            let xz = r.byte()?;
            let y = i16::from_be_bytes(r.take(2)?.try_into().unwrap());
            let kind = nonnegative(r)?;
            let data = if r.0.first() == Some(&0) {
                r.byte()?;
                None
            } else {
                let value = nbt::read(r)?;
                if !matches!(value, nbt::Value::Compound(_)) {
                    return Err(Error::Invalid("Java block entity compound"));
                }
                Some(value)
            };
            if i32::from(y) < self.min_y || i32::from(y) >= self.min_y + self.height {
                return Err(Error::Invalid("Java block entity Y"));
            }
            block_entities.push(BlockEntity {
                x: xz >> 4,
                y,
                z: xz & 15,
                kind,
                data,
            });
        }
        Ok(Chunk {
            x,
            z,
            heightmaps,
            sections,
            block_entities,
            light: Light::read(r, self.sections())?,
        })
    }
}
/// Signed 26/12/26-bit block position, in world X/Y/Z order.
pub fn block_position(p: i64) -> [i32; 3] {
    [
        (p >> 38) as i32,
        (p << 52 >> 52) as i32,
        (p << 26 >> 38) as i32,
    ]
}
fn section_updates(r: &mut Reader<'_>) -> Result<Vec<BlockUpdate>, Error> {
    let p = r.i64()?;
    let origin = [
        (p >> 42) as i32 * 16,
        (p << 44 >> 44) as i32 * 16,
        (p << 22 >> 42) as i32 * 16,
    ];
    let n = r.count(4096)?;
    let mut updates = Vec::with_capacity(n);
    for _ in 0..n {
        let p = r.varlong()?;
        let state =
            u32::try_from(p >> 12).map_err(|_| Error::Invalid("Java block state overflow"))?;
        if state > i32::MAX as u32 {
            return Err(Error::Invalid("Java block state overflow"));
        }
        updates.push(BlockUpdate {
            position: [
                origin[0] + ((p >> 8) & 15) as i32,
                origin[1] + (p & 15) as i32,
                origin[2] + ((p >> 4) & 15) as i32,
            ],
            state,
        });
    }
    Ok(updates)
}
