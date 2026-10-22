//! Typed Java facts handed to the client, without Bedrock IDs or synthetic packets.
use crate::{ProtocolError as Error, auth::Profile, nbt, wire::Reader};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
pub struct RegistryEntry {
    pub name: String,
    pub value: nbt::Value,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Configuration {
    pub registries: BTreeMap<String, Vec<RegistryEntry>>,
    pub features: Vec<String>,
    pub tags: BTreeMap<String, BTreeMap<String, Vec<i32>>>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Spawn {
    pub dimension_type: i32,
    pub world: String,
    pub hashed_seed: i64,
    pub game_mode: u8,
    pub previous_game_mode: i8,
    pub debug: bool,
    pub flat: bool,
    pub last_death: Option<(String, i64)>,
    pub portal_cooldown: i32,
    pub sea_level: i32,
}
#[derive(Clone, Debug, Serialize)]
pub struct World {
    pub entity_id: i32,
    pub hardcore: bool,
    pub worlds: Vec<String>,
    pub max_players: i32,
    pub view_distance: i32,
    pub simulation_distance: i32,
    pub reduced_debug: bool,
    pub respawn_screen: bool,
    pub limited_crafting: bool,
    pub spawn: Spawn,
    pub online_mode: bool,
    pub secure_chat: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Bootstrap {
    pub generation: u64,
    pub profile: Profile,
    pub server_session: uuid::Uuid,
    pub configuration: Configuration,
    pub world: World,
}
#[derive(Clone, Debug, Serialize)]
pub struct Teleport {
    pub id: i32,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub relative: u32,
}
/// Protocol-777 Relative bit order, verified against the pinned client's Relative enum.
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Relative {
    X,
    Y,
    Z,
    Yaw,
    Pitch,
    VelocityX,
    VelocityY,
    VelocityZ,
    RotateVelocity,
}
impl Relative {
    pub const fn mask(self) -> u32 {
        1 << self as u8
    }
}
pub fn registry(r: &mut Reader<'_>) -> Result<(String, Vec<RegistryEntry>), Error> {
    let name = r.string(32767)?;
    let count = r.count(262144)?;
    let mut entries = Vec::with_capacity(count);
    let mut names = std::collections::HashSet::new();
    for _ in 0..count {
        let name = r.string(32767)?;
        if !names.insert(name.clone()) {
            return Err(Error::Invalid("duplicate registry entry"));
        }
        if !r.bool()? {
            return Err(Error::Invalid(
                "omitted registry value without negotiated known pack",
            ));
        }
        entries.push(RegistryEntry {
            name,
            value: nbt::read(r)?,
        });
    }
    Ok((name, entries))
}
pub fn strings(r: &mut Reader<'_>) -> Result<Vec<String>, Error> {
    let n = r.count(65536)?;
    (0..n).map(|_| r.string(32767)).collect()
}
pub fn tags(r: &mut Reader<'_>) -> Result<BTreeMap<String, BTreeMap<String, Vec<i32>>>, Error> {
    let n = r.count(65536)?;
    let mut registries = BTreeMap::new();
    for _ in 0..n {
        let registry = r.string(32767)?;
        let n = r.count(65536)?;
        let mut tags = BTreeMap::new();
        for _ in 0..n {
            let name = r.string(32767)?;
            let n = r.count(262144)?;
            let mut ids = Vec::with_capacity(n);
            for _ in 0..n {
                let id = r.var()?;
                if id < 0 {
                    return Err(Error::Invalid("negative tag id"));
                }
                ids.push(id);
            }
            if tags.insert(name, ids).is_some() {
                return Err(Error::Invalid("duplicate tag"));
            }
        }
        if registries.insert(registry, tags).is_some() {
            return Err(Error::Invalid("duplicate tag registry"));
        }
    }
    Ok(registries)
}
impl Spawn {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            dimension_type: r.var()?,
            world: r.string(32767)?,
            hashed_seed: r.i64()?,
            game_mode: r.byte()?,
            previous_game_mode: r.byte()? as i8,
            debug: r.bool()?,
            flat: r.bool()?,
            last_death: if r.bool()? {
                Some((r.string(32767)?, r.i64()?))
            } else {
                None
            },
            portal_cooldown: r.var()?,
            sea_level: r.var()?,
        })
    }
}
impl World {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            entity_id: r.i32()?,
            hardcore: r.bool()?,
            worlds: strings(r)?,
            max_players: r.var()?,
            view_distance: r.var()?,
            simulation_distance: r.var()?,
            reduced_debug: r.bool()?,
            respawn_screen: r.bool()?,
            limited_crafting: r.bool()?,
            spawn: Spawn::read(r)?,
            online_mode: r.bool()?,
            secure_chat: r.bool()?,
        })
    }
}
impl Teleport {
    pub fn is_relative(&self, component: Relative) -> bool {
        self.relative & component.mask() != 0
    }
    pub fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        let position = Self {
            id: r.var()?,
            position: [r.f64()?, r.f64()?, r.f64()?],
            velocity: [r.f64()?, r.f64()?, r.f64()?],
            yaw: r.f32()?,
            pitch: r.f32()?,
            relative: r.i32()? as u32,
        };
        if position.relative & !(Relative::RotateVelocity.mask() * 2 - 1) != 0 {
            return Err(Error::Invalid("unknown Java player relative flags"));
        }
        Ok(position)
    }
}
impl Bootstrap {
    /// Rejects cross-registry dimension IDs before publishing authoritative facts.
    pub fn validate(&self) -> Result<(), Error> {
        let dimensions = self
            .configuration
            .registries
            .get("minecraft:dimension_type")
            .ok_or(Error::Invalid("missing dimension registry"))?;
        if self.world.spawn.dimension_type < 0
            || self.world.spawn.dimension_type as usize >= dimensions.len()
            || !self.world.worlds.contains(&self.world.spawn.world)
        {
            return Err(Error::Invalid("world/dimension identity"));
        }
        Ok(())
    }
}

/// Server-authoritative vital values; no client-side hunger or damage inference.
#[derive(Clone, Debug, Serialize)]
pub struct Vitals {
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
}
impl Vitals {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            health: r.f32()?,
            food: r.var()?,
            saturation: r.f32()?,
        })
    }
}
