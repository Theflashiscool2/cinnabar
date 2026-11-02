//! Session-owned Java objectives and scores; display selection is separate from storage.
use java_protocol::session::scoreboard as wire;
use std::collections::BTreeMap;
use world::NbtValue;

#[derive(Clone, Debug, PartialEq)]
pub enum NumberFormat {
    Blank,
    Styled(NbtValue),
    Fixed(NbtValue),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Objective {
    pub display: NbtValue,
    pub render: i32,
    pub format: Option<NumberFormat>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub owner: String,
    pub score: i32,
    pub display: Option<NbtValue>,
    pub format: Option<NumberFormat>,
}
#[derive(Debug, Default)]
pub struct State {
    objectives: BTreeMap<String, Objective>,
    scores: BTreeMap<(String, String), Row>,
    displays: BTreeMap<i32, String>,
    revision: u64,
}
fn format(f: wire::NumberFormat) -> NumberFormat {
    match f {
        wire::NumberFormat::Blank => NumberFormat::Blank,
        wire::NumberFormat::Styled(n) => NumberFormat::Styled(super::nbt::convert(n)),
        wire::NumberFormat::Fixed(n) => NumberFormat::Fixed(super::nbt::convert(n)),
    }
}
impl State {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Also invalidates formatted rows when team membership or prefixes change.
    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn clear(&mut self) {
        let revision = self.revision.wrapping_add(1);
        *self = Self {
            revision,
            ..Self::default()
        };
    }
    pub(super) fn admit(&mut self, packet: wire::Packet) {
        match packet {
            wire::Packet::Objective {
                name,
                method,
                display,
            } => match (method, display) {
                (1, _) => {
                    self.objectives.remove(&name);
                    self.scores.retain(|(o, _), _| o != &name);
                    self.displays.retain(|_, o| o != &name);
                }
                (0, Some((display, render, f))) if !self.objectives.contains_key(&name) => {
                    self.objectives.insert(
                        name,
                        Objective {
                            display: super::nbt::convert(display),
                            render,
                            format: f.map(format),
                        },
                    );
                }
                (2, Some((display, render, f))) => {
                    if let Some(o) = self.objectives.get_mut(&name) {
                        *o = Objective {
                            display: super::nbt::convert(display),
                            render,
                            format: f.map(format),
                        };
                    }
                }
                _ => return,
            },
            wire::Packet::Display { slot, objective } => {
                if objective.is_empty() || !self.objectives.contains_key(&objective) {
                    self.displays.remove(&slot);
                } else {
                    self.displays.insert(slot, objective);
                }
            }
            wire::Packet::Score {
                owner,
                objective,
                score,
                display,
                format: f,
            } => {
                if !self.objectives.contains_key(&objective) {
                    return;
                }
                self.scores.insert(
                    (objective, owner.clone()),
                    Row {
                        owner,
                        score,
                        display: display.map(super::nbt::convert),
                        format: f.map(format),
                    },
                );
            }
            wire::Packet::Reset { owner, objective } => self
                .scores
                .retain(|(o, n), _| n != &owner || objective.as_ref().is_some_and(|v| v != o)),
        }
        self.touch();
    }
    /// Java's sidebar selects the viewer's team-colour slot first, then the ordinary sidebar.
    /// Scores descend, ties use case-insensitive holder order; hidden holders start with '#'.
    pub fn sidebar(&self, color: Option<char>, limit: usize) -> Option<(&Objective, Vec<&Row>)> {
        let team = color
            .and_then(|c| c.to_digit(16))
            .and_then(|n| self.displays.get(&(3 + n as i32)));
        let name = team.or_else(|| self.displays.get(&1))?;
        let objective = self.objectives.get(name)?;
        let mut rows = self
            .scores
            .iter()
            .filter(|((o, n), _)| o == name && !n.starts_with('#'))
            .map(|(_, r)| r)
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.owner.to_lowercase().cmp(&b.owner.to_lowercase()))
        });
        rows.truncate(limit);
        Some((objective, rows))
    }
}
#[cfg(test)]
mod tests;
