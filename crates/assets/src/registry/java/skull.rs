//! Native fixed mob skull identities; player-profile skins have separate resource ownership.
use super::{BlockState, source::Error};
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Cube,
    Human,
    Dragon,
    Piglin,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Skull {
    pub texture: &'static str,
    pub shape: Shape,
    pub powered: bool,
    pub wall: bool,
    pub rotation: u8,
}
impl Skull {
    pub fn from_state(state: &BlockState) -> Result<Option<Self>, Error> {
        let Some(name) = state.name.strip_prefix("minecraft:") else {
            return Ok(None);
        };
        let (texture, shape) = match name {
            "skeleton_skull" | "skeleton_wall_skull" => {
                ("textures/entity/skeleton/skeleton", Shape::Cube)
            }
            "wither_skeleton_skull" | "wither_skeleton_wall_skull" => {
                ("textures/entity/skeleton/wither_skeleton", Shape::Cube)
            }
            "zombie_head" | "zombie_wall_head" => ("textures/entity/zombie/zombie", Shape::Human),
            "creeper_head" | "creeper_wall_head" => {
                ("textures/entity/creeper/creeper", Shape::Cube)
            }
            "dragon_head" | "dragon_wall_head" => {
                ("textures/entity/enderdragon/dragon", Shape::Dragon)
            }
            "piglin_head" | "piglin_wall_head" => ("textures/entity/piglin/piglin", Shape::Piglin),
            _ => return Ok(None),
        };
        let wall = name.contains("_wall_");
        let invalid = || {
            Error::Model(format!(
                "invalid native Java skull properties: {}",
                state.name
            ))
        };
        if !matches!(
            state.properties.get("powered").map(String::as_str),
            Some("true" | "false")
        ) {
            return Err(invalid());
        }
        let rotation = if wall {
            match state.properties.get("facing").map(String::as_str) {
                Some("south") => 0,
                Some("west") => 4,
                Some("north") => 8,
                Some("east") => 12,
                _ => return Err(invalid()),
            }
        } else {
            state
                .properties
                .get("rotation")
                .and_then(|s| s.parse::<u8>().ok())
                .filter(|n| *n < 16)
                .ok_or_else(invalid)?
        };
        Ok(Some(Self {
            texture,
            shape,
            powered: state.properties.get("powered").is_some_and(|s| s == "true"),
            wall,
            rotation,
        }))
    }
}
