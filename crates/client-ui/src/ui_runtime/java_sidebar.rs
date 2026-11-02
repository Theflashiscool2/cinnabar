//! Native sidebar projection into the existing JSON-UI HUD. No Bedrock packet synthesis.
use super::{
    UiRuntime,
    presentation::{
        nametags::java::text,
        retained_hud::{
            MAX_PRESENTED_SCOREBOARD_ROWS, PresentedScoreValue, PresentedScoreboard,
            PresentedScoreboardRow, ScoreboardPresentationScope,
        },
    },
};
use client_world::java::{WorldState, scoreboard::NumberFormat};
use world::NbtValue;

#[derive(Clone, Debug, Default)]
pub(super) struct SidebarState {
    key: Option<(u64, u64)>,
    pub revision: u64,
    pub projection: Option<PresentedScoreboard>,
    pub scores: Vec<i32>,
    pub unsupported_text: bool,
}
impl UiRuntime {
    /// Updates only on authoritative score/team changes; stale sessions cannot replace the HUD.
    pub fn publish_java_sidebar(&mut self, state: &WorldState) -> bool {
        if state.generation() != self.session.session_id || self.session.java_hud.is_none() {
            return false;
        }
        let key = (state.generation(), state.scoreboard.revision());
        let sidebar = &mut self.session.java_sidebar;
        if sidebar.key == Some(key) {
            return true;
        }
        sidebar.key = Some(key);
        sidebar.revision = sidebar.revision.wrapping_add(1);
        sidebar.projection = None;
        sidebar.scores.clear();
        sidebar.unsupported_text = false;
        let viewer = state
            .bootstrap
            .as_ref()
            .map(|b| b.profile.name.as_str())
            .unwrap_or("");
        let color = state.players.team_parameters(viewer).and_then(|p| p.color);
        let Some((objective, rows)) = state
            .scoreboard
            .sidebar(color, MAX_PRESENTED_SCOREBOARD_ROWS)
        else {
            return true;
        };
        sidebar.scores = rows.iter().map(|row| row.score).collect();
        let projected = project(objective, &rows, &state.players);
        match projected {
            Ok(projection) => sidebar.projection = Some(projection),
            Err(()) => {
                sidebar.unsupported_text = true;
                sidebar.scores.clear();
            }
        }
        true
    }
    /// Bounded inspection of the same rows consumed by the real HUD, on explicit request only.
    pub fn java_sidebar_snapshot(&self) -> Option<(String, Vec<(String, i32)>)> {
        let sidebar = &self.session.java_sidebar;
        let projection = sidebar.projection.as_ref()?;
        Some((
            projection.title.to_string(),
            projection
                .rows
                .iter()
                .zip(&sidebar.scores)
                .map(|(row, score)| (row.label.to_string(), *score))
                .collect(),
        ))
    }
    /// Translation/custom-font components remain explicit gaps rather than silent substitutions.
    pub fn java_sidebar_has_unsupported_text(&self) -> bool {
        self.session.java_sidebar.unsupported_text
    }
}

fn project(
    objective: &client_world::java::scoreboard::Objective,
    rows: &[&client_world::java::scoreboard::Row],
    players: &world::java::roster::Roster,
) -> Result<PresentedScoreboard, ()> {
    let title = text::component_text(&objective.display, None)?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let owner = NbtValue::String(row.owner.clone().into());
        let label = text::component_text(
            row.display.as_ref().unwrap_or(&owner),
            players.team_parameters(&row.owner),
        )?;
        let value = match row.format.as_ref().or(objective.format.as_ref()) {
            Some(NumberFormat::Blank) => "".into(),
            Some(NumberFormat::Fixed(value)) => text::component_text(value, None)?,
            format => {
                let mut style = match format {
                    Some(NumberFormat::Styled(NbtValue::Compound(c))) => c.clone(),
                    Some(NumberFormat::Styled(_)) => return Err(()),
                    _ => {
                        let mut c = world::NbtCompound::default();
                        c.insert("color", NbtValue::String("red".into()));
                        c
                    }
                };
                style.insert("text", NbtValue::String(row.score.to_string().into()));
                text::component_text(&NbtValue::Compound(style), None)?
            }
        };
        result.push(PresentedScoreboardRow {
            label,
            value: PresentedScoreValue::Text(value),
        });
    }
    Ok(PresentedScoreboard {
        scope: ScoreboardPresentationScope::HudSidebar,
        title,
        rows: result,
    })
}

#[cfg(test)]
mod tests;
