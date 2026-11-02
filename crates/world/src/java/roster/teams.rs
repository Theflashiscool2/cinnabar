//! Session-owned team membership and visibility. No entity ID is used as a player-list identity.
use crate::NbtValue;
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Always,
    Never,
    OtherTeamsHidden,
    OwnTeamHidden,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collision {
    Always,
    Never,
    OtherTeams,
    OwnTeam,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Parameters {
    pub display: NbtValue,
    pub prefix: NbtValue,
    pub suffix: NbtValue,
    pub visibility: Visibility,
    pub collision: Collision,
    pub color: Option<char>,
    pub options: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Create,
    Remove,
    Update,
    Join,
    Leave,
    Unknown,
}
#[derive(Clone, Debug)]
pub struct Update {
    pub team: String,
    pub action: Action,
    pub parameters: Option<Parameters>,
    pub members: Vec<String>,
}
#[derive(Debug, Default)]
pub(super) struct State {
    teams: BTreeMap<String, Arc<Parameters>>,
    members: BTreeMap<String, String>,
    pub ignored: u64,
}
impl State {
    pub fn parameters(&self, owner: &str) -> Option<&Parameters> {
        self.teams.get(self.members.get(owner)?).map(AsRef::as_ref)
    }
    pub fn name(&self, owner: &str) -> Option<&str> {
        self.members.get(owner).map(String::as_str)
    }
    pub fn apply(&mut self, update: Update) {
        let Update {
            team,
            action,
            parameters,
            members,
        } = update;
        match action {
            Action::Create | Action::Update => {
                if action == Action::Update && !self.teams.contains_key(&team) {
                    self.ignored = self.ignored.saturating_add(1);
                    return;
                }
                let Some(parameters) = parameters else {
                    self.ignored = self.ignored.saturating_add(1);
                    return;
                };
                self.teams.insert(team.clone(), Arc::new(parameters));
                if action == Action::Create {
                    for member in members {
                        self.members.insert(member, team.clone());
                    }
                }
            }
            Action::Remove => {
                self.teams.remove(&team);
                self.members.retain(|_, t| t != &team);
            }
            Action::Join => {
                if !self.teams.contains_key(&team) {
                    self.ignored = self.ignored.saturating_add(1);
                    return;
                }
                for member in members {
                    self.members.insert(member, team.clone());
                }
            }
            Action::Leave => {
                for member in members {
                    if self.members.get(&member) == Some(&team) {
                        self.members.remove(&member);
                    }
                }
            }
            Action::Unknown => self.ignored = self.ignored.saturating_add(1),
        }
    }
    /// Native LivingEntityRenderer lets an unteamed viewer see either conditional rule.
    pub fn visible(&self, owner: &str, viewer: &str) -> bool {
        let Some(params) = self.parameters(owner) else {
            return true;
        };
        let same = self.name(owner) == self.name(viewer);
        match params.visibility {
            Visibility::Always => true,
            Visibility::Never => false,
            Visibility::OtherTeamsHidden => self.name(viewer).is_none() || same,
            Visibility::OwnTeamHidden => self.name(viewer).is_none() || !same,
        }
    }

    pub fn sees_invisible(&self, owner: &str, viewer: &str) -> bool {
        self.parameters(owner).is_some_and(|p| p.options & 2 != 0)
            && self.name(owner) == self.name(viewer)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn update(team: &str, action: Action, members: &[&str]) -> Update {
        Update {
            team: team.into(),
            action,
            parameters: Some(Parameters {
                display: NbtValue::String("A".into()),
                prefix: NbtValue::String("[A]".into()),
                suffix: NbtValue::String("".into()),
                visibility: Visibility::OtherTeamsHidden,
                collision: Collision::Always,
                color: Some('c'),
                options: 2,
            }),
            members: members.iter().map(|s| s.to_string()).collect(),
        }
    }
    #[test]
    fn membership_is_unique_and_wrong_team_removal_cannot_erase_current_membership() {
        let mut s = State::default();
        s.apply(update("a", Action::Create, &["Alice", "Viewer"]));
        s.apply(update("b", Action::Create, &["Bob"]));
        assert!(s.visible("Alice", "Viewer"));
        assert!(!s.visible("Bob", "Viewer"));
        s.apply(update("a", Action::Join, &["Bob"]));
        assert!(s.visible("Bob", "Viewer"));
        s.apply(update("b", Action::Leave, &["Bob"]));
        assert_eq!(s.name("Bob"), Some("a"));
        s.apply(update("a", Action::Remove, &[]));
        assert!(s.name("Alice").is_none() && s.name("Bob").is_none());
    }
    #[test]
    fn duplicate_creation_updates_parameters_without_losing_existing_members() {
        let mut s = State::default();
        s.apply(update("a", Action::Create, &["Alice"]));
        let mut u = update("a", Action::Create, &["Bob"]);
        u.parameters.as_mut().unwrap().visibility = Visibility::Never;
        s.apply(u);
        assert_eq!(s.name("Alice"), Some("a"));
        assert!(!s.visible("Alice", "Bob"));
    }
    #[test]
    fn unknown_updates_do_not_invent_teams_or_visibility() {
        let mut s = State::default();
        s.apply(update("missing", Action::Update, &["Alice"]));
        s.apply(update("missing", Action::Join, &["Alice"]));
        assert!(s.name("Alice").is_none());
        assert_eq!(s.ignored, 2);
    }

    #[test]
    fn conditional_visibility_distinguishes_an_unteamed_viewer_from_an_enemy() {
        let mut s = State::default();
        s.apply(update("a", Action::Create, &["Alice", "Ally"]));
        s.apply(update("b", Action::Create, &["Enemy"]));
        assert!(s.visible("Alice", "Unteamed"));
        assert!(s.visible("Alice", "Ally"));
        assert!(!s.visible("Alice", "Enemy"));
        assert!(s.sees_invisible("Alice", "Ally"));
        assert!(!s.sees_invisible("Alice", "Unteamed"));
        let mut u = update("a", Action::Update, &[]);
        u.parameters.as_mut().unwrap().visibility = Visibility::OwnTeamHidden;
        s.apply(u);
        assert!(s.visible("Alice", "Unteamed"));
        assert!(!s.visible("Alice", "Ally"));
        assert!(s.visible("Alice", "Enemy"));
    }
}
