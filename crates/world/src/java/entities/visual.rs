//! Native entity appearance facts; no wire metadata IDs reach render code.
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PigModel {
    Normal,
    Cold,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PigAppearance {
    pub name: Arc<str>,
    pub model: PigModel,
    pub adult_texture: Arc<str>,
    pub baby_texture: Arc<str>,
}
#[derive(Clone, Debug, Default)]
pub struct MetadataGap {
    pub index: u8,
    pub serializer: Arc<str>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerLayers {
    pub hat: bool,
    pub jacket: bool,
    pub left_sleeve: bool,
    pub right_sleeve: bool,
    pub left_pants: bool,
    pub right_pants: bool,
    pub cape: bool,
}
#[derive(Clone, Debug)]
pub struct Visual {
    pub flags: u8,
    pub swing: super::animation::swing::State,
    pub effects: std::collections::BTreeMap<String, super::super::dynamics::Effect>,
    /// LivingEntity use/offhand/spin bits, admitted only from native authoritative updates.
    pub use_flags: u8,
    pub use_revision: u64,
    pub scale: f32,
    pub pose: Arc<str>,
    pub baby: bool,
    pub health: Option<f32>,
    pub pig: Option<Arc<PigAppearance>>,
    pub player: Option<Arc<super::super::roster::Profile>>,
    pub player_layers: PlayerLayers,
    /// Avatar.DATA_PLAYER_MAIN_HAND; native constructor defaults to RIGHT.
    pub left_handed: bool,
    /// A missed representation persists until a fresh spawn, since later packets are deltas.
    pub incomplete: Option<MetadataGap>,
}
impl Default for Visual {
    fn default() -> Self {
        // Pinned Entity constructor and AgeableMob.defineSynchedData defaults. These are
        // native defaults, not claims that an omitted required variant was received.
        Self {
            flags: 0,
            swing: Default::default(),
            effects: Default::default(),
            use_flags: 0,
            use_revision: 0,
            scale: 1.,
            pose: "standing".into(),
            baby: false,
            health: None,
            pig: None,
            player: None,
            player_layers: PlayerLayers::default(),
            left_handed: false,
            incomplete: None,
        }
    }
}
impl Visual {
    pub fn observe_use(&mut self, flags: u8) -> Result<(), &'static str> {
        let revision = self
            .use_revision
            .checked_add(1)
            .ok_or("Java use revision exhausted")?;
        self.use_flags = flags;
        self.use_revision = revision;
        Ok(())
    }
    pub fn complete_use(&mut self) -> Result<(), &'static str> {
        self.observe_use(self.use_flags & !1)
    }
    pub fn invisible(&self) -> bool {
        self.flags & 0x20 != 0
    }
    pub fn glowing(&self) -> bool {
        self.flags & 0x40 != 0
    }
}
