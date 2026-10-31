//! Version-pinned fluid/host facts, exported from native Java block states.
use super::source::Error;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Empty,
    Water,
    Lava,
}

#[derive(Clone, Debug, Deserialize)]
pub struct State {
    pub id: u32,
    pub kind: Kind,
    pub height: f32,
    pub falling: bool,
    pub solid: bool,
    pub solid_render: bool,
    pub flow_blocking: bool,
    pub overlay: bool,
    pub invisible: bool,
    pub emission: u8,
    /// Neighbor-facing coverage in -X,+X,-Y,+Y,-Z,+Z order; side values are height thresholds.
    pub occlusion: [f32; 6],
}
impl State {
    /// Tests whether this neighbor covers the complete fluid face at the requested height.
    pub fn covers(&self, direction: usize, height: f32) -> bool {
        let coverage = self.occlusion[direction];
        coverage > 0.
            && match direction {
                3 => height >= 1.,
                2 => true,
                _ => coverage >= height,
            }
    }
}

#[derive(Debug)]
pub struct Table(Box<[State]>);
impl Table {
    /// Verifies the complete report before admitting any state or resource identity.
    pub fn load(bytes: &[u8], hash: &str, count: usize) -> Result<Self, Error> {
        if format!("{:x}", Sha256::digest(bytes)) != hash {
            return Err(Error::Hash);
        }
        #[derive(Deserialize)]
        struct Report {
            schema: u32,
            states: Vec<State>,
        }
        let report: Report = serde_json::from_slice(bytes)?;
        if report.schema != 1 || report.states.len() != count || count == 0 {
            return Err(Error::Model("Java fluid report schema/state count".into()));
        }
        for (id, state) in report.states.iter().enumerate() {
            if state.id as usize != id
                || !state.height.is_finite()
                || !(0. ..=1.).contains(&state.height)
                || (state.kind == Kind::Empty) != (state.height == 0.)
                || state.emission > 15
                || state
                    .occlusion
                    .iter()
                    .any(|h| !h.is_finite() || !(0. ..=1.).contains(h))
            {
                return Err(Error::Model("Java fluid report invalid state".into()));
            }
        }
        Ok(Self(report.states.into()))
    }
    pub fn state(&self, id: u32) -> Option<&State> {
        self.0.get(id as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_validates_identity_bounds_and_neighbor_face_coverage() {
        let mut state = serde_json::json!({"id":0,"kind":"water","height":0.8888889,"falling":false,
            "solid":false,"solid_render":false,"flow_blocking":false,"overlay":false,
            "invisible":true,"emission":0,"occlusion":[0.5,0.5,1.0,1.0,0.0,0.0]});
        let load = |s: &serde_json::Value| {
            let bytes = serde_json::to_vec(&serde_json::json!({"schema":1,"states":[s]})).unwrap();
            Table::load(&bytes, &format!("{:x}", Sha256::digest(&bytes)), 1)
        };
        let table = load(&state).unwrap();
        let cell = table.state(0).unwrap();
        assert!(cell.covers(0, 0.5));
        assert!(!cell.covers(0, 0.501));
        assert!(cell.covers(2, 0.8));
        assert!(!cell.covers(3, 0.8));
        assert!(cell.covers(3, 1.));
        assert!(table.state(1).is_none());
        state["id"] = 1.into();
        assert!(load(&state).is_err());
        state["id"] = 0.into();
        state["kind"] = "empty".into();
        assert!(load(&state).is_err());
        state["kind"] = "water".into();
        state["height"] = 1.1.into();
        assert!(load(&state).is_err());
    }
}
