//! Native copper statue identity; model parts come from the verified Java model exporter.
use super::{BlockState, source::Error};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statue {
    pub texture: &'static str,
    pub model: &'static str,
    /// South, west, north, east.
    pub turn: u8,
}
impl Statue {
    pub fn from_state(state: &BlockState) -> Result<Option<Self>, Error> {
        let name = state.name.strip_prefix("minecraft:").unwrap_or("");
        let bare = name.strip_prefix("waxed_").unwrap_or(name);
        let texture = match bare {
            "copper_golem_statue" => "textures/entity/copper_golem/copper_golem",
            "exposed_copper_golem_statue" => "textures/entity/copper_golem/copper_golem_exposed",
            "weathered_copper_golem_statue" => {
                "textures/entity/copper_golem/copper_golem_weathered"
            }
            "oxidized_copper_golem_statue" => "textures/entity/copper_golem/copper_golem_oxidized",
            _ => return Ok(None),
        };
        let invalid = || {
            Error::Model(format!(
                "invalid native Java statue properties: {}",
                state.name
            ))
        };
        let model = match state
            .properties
            .get("copper_golem_pose")
            .map(String::as_str)
        {
            Some("standing") => "statue_standing",
            Some("running") => "statue_running",
            Some("sitting") => "statue_sitting",
            Some("star") => "statue_star",
            _ => return Err(invalid()),
        };
        let turn = match state.properties.get("facing").map(String::as_str) {
            Some("south") => 0,
            Some("west") => 1,
            Some("north") => 2,
            Some("east") => 3,
            _ => return Err(invalid()),
        };
        if !matches!(
            state.properties.get("waterlogged").map(String::as_str),
            Some("true" | "false")
        ) {
            return Err(invalid());
        }
        Ok(Some(Self {
            texture,
            model,
            turn,
        }))
    }
}
