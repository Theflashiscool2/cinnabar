//! Native entity appearance facts; no wire metadata IDs reach render code.
use std::sync::Arc;

/// Pinned LivingEntity.animateHurt duration, measured in native visual ticks.
pub const HURT_TICKS: u8 = 10;

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
/// Exact built-in villager identity and synchronized presentation facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VillagerAppearance {
    pub variant: Arc<str>,
    pub profession: Arc<str>,
    pub level: i32,
    pub unhappy_ticks: i32,
    pub finalized: bool,
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
/// Remote LivingEntity's captured used item and remaining client ticks. Unknown specialized
/// durations stay absent; they are never guessed from the active-use flag or item count.
#[derive(Clone, Debug)]
pub struct RemoteUse {
    pub item: Arc<str>,
    /// Slot plus appearance receipt identifies the captured native ItemStack reference.
    pub item_token: (u8, u64),
    pub observed_item: Option<Arc<str>>,
    pub observed_token: Option<(u8, u64)>,
    pub remaining: Option<i32>,
}
#[derive(Clone, Debug)]
pub struct Visual {
    pub flags: u8,
    pub no_gravity: bool,
    /// Present only for native dropped-item presentation, derived from the pinned entity report.
    pub item_dimensions: Option<[f32; 2]>,
    pub hurt_ticks: u8,
    pub custom_name: Option<Arc<crate::NbtValue>>,
    pub custom_name_visible: bool,
    pub name_height: f32,
    pub armor_stand_flags: Option<u8>,
    pub swing: super::animation::swing::State,
    pub effects: std::collections::BTreeMap<String, super::super::dynamics::Effect>,
    /// LivingEntity use/offhand/spin bits, admitted only from native authoritative updates.
    pub use_flags: u8,
    pub use_revision: u64,
    pub remote_use: Option<RemoteUse>,
    pub scale: f32,
    pub pose: Arc<str>,
    pub baby: bool,
    pub health: Option<f32>,
    pub pig: Option<Arc<PigAppearance>>,
    pub villager: Option<VillagerAppearance>,
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
            no_gravity: false,
            item_dimensions: None,
            hurt_ticks: 0,
            custom_name: None,
            custom_name_visible: false,
            name_height: 0.,
            armor_stand_flags: None,
            swing: Default::default(),
            effects: Default::default(),
            use_flags: 0,
            use_revision: 0,
            remote_use: None,
            scale: 1.,
            pose: "standing".into(),
            baby: false,
            health: None,
            pig: None,
            villager: None,
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
        if flags & 1 == 0 {
            self.remote_use = None;
        }
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
