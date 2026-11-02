//! Native player labels. Team visibility and native identities never pass through Bedrock actors.
use super::{NametagAnchor, NametagView};
use std::{collections::BTreeMap, sync::Arc};
use world::java::{
    entities::Identity,
    hud::GameMode,
    roster::{Profile, teams::Parameters},
};
#[cfg(test)]
mod tests;
mod text;

struct Cached {
    profile: Arc<Profile>,
    team: Option<Parameters>,
    lines: Result<Arc<[Arc<str>]>, ()>,
    seen: bool,
}
#[derive(Default)]
pub struct Cache {
    entries: BTreeMap<Identity, Cached>,
    anchors: Vec<NametagAnchor>,
    /// Labels whose received component styling/content needs a further text consumer.
    pub unsupported_text: usize,
}
impl Cache {
    /// Retires all connection-owned text when its edition or world stops presenting.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.anchors.clear();
        self.unsupported_text = 0;
    }
    /// Reuses the retired frame buffer after shared GPU publication takes the replacement.
    pub fn recycle_anchors(&mut self, mut anchors: Vec<NametagAnchor>) {
        anchors.clear();
        if anchors.capacity() >= self.anchors.capacity() {
            self.anchors = anchors;
        }
    }
    /// Selects labels from the same immutable authority and interpolation used by native bodies.
    pub fn project(
        &mut self,
        state: &client_world::java::WorldState,
        view: NametagView,
        fraction: f32,
    ) -> Vec<NametagAnchor> {
        let Some(bootstrap) = &state.bootstrap else {
            self.clear();
            return Vec::new();
        };
        self.project_players(
            &state.entities,
            &state.players,
            &bootstrap.profile.name,
            state.status.mode,
            view,
            fraction,
        )
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "independent native authority and camera inputs"
    )]
    fn project_players(
        &mut self,
        entities: &world::java::entities::Entities,
        players: &world::java::roster::Roster,
        viewer: &str,
        mode: Option<GameMode>,
        view: NametagView,
        fraction: f32,
    ) -> Vec<NametagAnchor> {
        self.unsupported_text = 0;
        for c in self.entries.values_mut() {
            c.seen = false;
        }
        let mut tags = std::mem::take(&mut self.anchors);
        for e in entities.iter() {
            let Some(profile) = &e.visual.player else {
                continue;
            };
            if e.visual.incomplete.is_some() || e.visual.health.is_some_and(|h| h <= 0.) {
                continue;
            }
            if !players.player_name_visible(
                &profile.name,
                viewer,
                e.visual.invisible(),
                mode == Some(GameMode::Spectator),
            ) {
                continue;
            }
            let Some(motion) = &e.presentation else {
                continue;
            };
            let feet = glam::DVec3::from_array(motion.sample(fraction).pose.position).as_vec3();
            let sneaking = e.visual.flags & 2 != 0;
            let limit = if sneaking { 32.0 } else { 64.0 };
            let distance = view.camera_position.distance_squared(feet);
            if !distance.is_finite() || distance >= limit * limit {
                continue;
            }
            // Other poses require their native attachment points before labels can use them.
            let height = match e.visual.pose.as_ref() {
                "standing" => super::DEFAULT_HEIGHT,
                "crouching" => 1.5,
                _ => continue,
            };
            let team = players.team_parameters(&profile.name);
            let cached = self.entries.entry(e.identity).or_insert_with(|| Cached {
                profile: Arc::clone(profile),
                team: team.cloned(),
                lines: text::player(&profile.name, team).map(|text| super::nametag_lines(&text)),
                seen: true,
            });
            if !Arc::ptr_eq(&cached.profile, profile) || cached.team.as_ref() != team {
                cached.profile = Arc::clone(profile);
                cached.team = team.cloned();
                cached.lines =
                    text::player(&profile.name, team).map(|text| super::nametag_lines(&text));
            }
            cached.seen = true;
            let Ok(lines) = &cached.lines else {
                self.unsupported_text += 1;
                continue;
            };
            // Pinned Avatar.POSES NAME_TAG attachments plus SubmitNodeCollection's clearance.
            let position = feet + glam::Vec3::Y * (height + super::HEAD_CLEARANCE);
            if let Some(tag) = NametagAnchor::world_label(
                e.identity.incarnation,
                position,
                Arc::clone(lines),
                view,
                sneaking,
                if sneaking { 0.5 } else { 1.0 },
            ) {
                tags.push(tag);
            }
        }
        self.entries.retain(|_, c| c.seen);
        tags
    }
}
