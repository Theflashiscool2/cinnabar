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
#[derive(Clone, Debug)]
pub struct Visual {
    pub flags: u8,
    pub pose: Arc<str>,
    pub baby: bool,
    pub health: Option<f32>,
    pub pig: Option<Arc<PigAppearance>>,
    /// A missed representation persists until a fresh spawn, since later packets are deltas.
    pub incomplete: Option<MetadataGap>,
}
impl Default for Visual {
    fn default() -> Self {
        // Pinned Entity constructor and AgeableMob.defineSynchedData defaults. These are
        // native defaults, not claims that an omitted required variant was received.
        Self {
            flags: 0,
            pose: "standing".into(),
            baby: false,
            health: None,
            pig: None,
            incomplete: None,
        }
    }
}
impl Visual {
    pub fn invisible(&self) -> bool {
        self.flags & 0x20 != 0
    }
    pub fn glowing(&self) -> bool {
        self.flags & 0x40 != 0
    }
}
