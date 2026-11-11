//! Opt-in correlated progress, without server text, item components, identities or positions.
//! One log row per second; no I/O, protocol or authority decisions here.
use crate::session::{
    SessionController, java_entities, java_interaction, java_movement, java_render, java_special,
};
use serde_json::{Value, json};
use std::time::Duration;
use web_time::Instant;

#[derive(Default)]
pub(super) struct Trace {
    scope: Option<(u64, u64)>,
    started: Option<Instant>,
    last: Option<(Instant, Option<[f64; 3]>, u64, u64)>,
}
impl Trace {
    pub(super) fn sample(
        &mut self,
        controller: Option<&SessionController>,
        interaction: Option<&java_interaction::Interaction>,
        movement: Option<&java_movement::Movement>,
        terrain: Option<&java_render::Presentation>,
        entities: Option<&java_entities::Presentation>,
        special: Option<&java_special::Presentation>,
    ) {
        let Some(c) = controller.filter(|c| c.java.is_some() && c.java_telemetry.is_some()) else {
            *self = Self::default();
            return;
        };
        let now = Instant::now();
        let scope = c
            .java_world
            .terrain
            .dimension()
            .map(|d| (d.session, d.epoch));
        if scope != self.scope {
            *self = Self {
                scope,
                ..Default::default()
            };
        }
        if self
            .last
            .is_some_and(|(last, ..)| now.saturating_duration_since(last) < Duration::from_secs(1))
        {
            return;
        }
        let elapsed = now.saturating_duration_since(*self.started.get_or_insert(now));
        let position = c.java_world.player_motion.map(|m| m.position);
        let ticks = movement.map_or(0, |m| m.0.ticks);
        let service_ticks = movement.map_or(0, |m| m.1.ticks);
        let delta = self.last.map(|(last, old, t, st)| json!({
            "sample_ms":now.saturating_duration_since(last).as_millis(),
            "simulation_ticks":ticks.saturating_sub(t),"between_frame_ticks":service_ticks.saturating_sub(st),
            "displacement":old.zip(position).map(|(a,b)|std::array::from_fn::<_,3,_>(|i|b[i]-a[i]))}));
        self.last = Some((now, position, ticks, service_ticks));
        let inventory = &c.java_world.inventory;
        let held = inventory.held().and_then(|s| s.as_ref());
        let probes = position.map(|p| {
            let cell = p.map(|v| v.floor() as i32);
            [-1, 0, 1].map(|dy| {
                c.java_world
                    .terrain
                    .block([cell[0], cell[1].saturating_add(dy), cell[2]])
                    .map(|id| id.0)
            })
        });
        let telemetry = c.java_telemetry.as_ref().expect("enabled above").snapshot();
        let last = |key: &str| telemetry[key].as_array().and_then(|v| v.last()).cloned();
        let row = json!({"schema":1,"elapsed_ms":elapsed.as_millis(),"scope":scope,"delta":delta,
            "columns":c.java_world.terrain.columns().count(),"block_updates":c.java_world.block_updates,
            "loading":terrain.map(|p|p.telemetry_progress()),"entities":entities.map(|p|p.telemetry_progress()),
            "special":special.map(|p|p.telemetry_progress()),"block_probes_below_feet_above":probes,
            "movement":movement.map(|m|json!({"ticks":ticks,"between_frame_ticks":service_ticks,
                "on_ground":m.0.body.on_ground,"horizontal_collision":m.0.body.horizontal_collision,
                "crouched":m.0.body.crouched,"sprinting":m.0.body.sprinting,"swimming":m.0.body.swimming,
                "pending":m.0.pending().is_some(),"backlog_ticks":m.0.backlog_ticks(),"blocked":m.0.blocked.is_some()})),
            "velocity":c.java_world.player_motion.map(|m|m.velocity),
            "interaction":interaction.map(|i|json!({"using":i.0.using_item(),"use_ticks":i.0.use_ticks(),
                "pending":i.0.pending().is_some(),"blocked":i.0.blocked.is_some(),
                "mining_progress":i.0.mining_progress().map(|(_,p)|p),"actions":i.0.queued_actions})),
            "inventory":{"selected":inventory.selected,"held_known":inventory.held().is_some(),
                "held_item":held.map(|s|s.item.as_str()),"held_count":held.map(|s|s.count),
                "wire_revision":inventory.wire_revision,"authority_revision":inventory.authority_revision,
                "container":inventory.opened.as_ref().map(|m|m.container),
                "container_revision":inventory.opened.as_ref().and_then(|m|m.state),"error_present":inventory.error.is_some()},
            "last_use_stop":last("recent_use_stops"),"last_inventory_refusal":last("recent_inventory_refusals"),
            "last_server_impulse":last("recent_server_impulses"),
            "limitations":"Sampled progress, not server action acceptance or GPU timing; deferred explosions remain unsupported."});
        bevy::log::debug!(target:"java_session", runtime=%row, "Java runtime diagnostic");
    }
}
