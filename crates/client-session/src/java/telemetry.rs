//! Opt-in, bounded observation only. A write/ACK is never evidence of server acceptance.
use java_protocol::session::{Command, interaction::Action};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

const RECENT: usize = 32;
/// Fixed publication stages only; never includes item, profile or server content.
#[derive(Clone, Copy, Debug)]
pub enum HandIssue {
    Owners,
    Hud,
    Loading,
    Visibility,
    Geometry,
    Position,
    Light,
    EmptyFrame,
    Publication,
}
impl HandIssue {
    fn key(self) -> &'static str {
        match self {
            Self::Owners => "missing_hand_owners",
            Self::Hud => "missing_hud",
            Self::Loading => "world_not_ready",
            Self::Visibility => "visibility_gate",
            Self::Geometry => "geometry_rejected",
            Self::Position => "missing_position",
            Self::Light => "missing_light",
            Self::EmptyFrame => "empty_geometry_frame",
            Self::Publication => "publication_rejected",
        }
    }
}
/// Diagnostic threshold, not a vanilla physics constant or a release performance gate.
const SPIKE: Duration = Duration::from_millis(50);
macro_rules! counters {
    ($($name:ident => ($key:literal, [$($item:literal),*])),* $(,)?) => {
        #[derive(Clone, Copy, Debug)]
        pub enum Counter { $($name),* }
        impl Counter {
            const ALL: &'static [Self] = &[$(Self::$name),*];
            fn key(self) -> &'static str { match self { $(Self::$name => $key),* } }
            fn items(self) -> &'static [&'static str] { match self { $(Self::$name => &[$($item),*]),* } }
        }
    }
}
counters! {
    Corrections => ("server_position_corrections", ["A06","A07","A08","A10","A11","P10"]),
    InitialPositions => ("initial_server_positions", ["A11","I01"]),
    TeleportsRefused => ("teleport_admission_refused", ["A11","F05"]),
    BlockRequests => ("block_action_requests_queued", ["C00","C04","C07","C10"]),
    BlockAcks => ("block_sequence_acks_admitted", ["C00","C10"]),
    BlockUpdates => ("authoritative_block_updates_admitted", ["C00","C04","C07","C10"]),
    PredictedBlocks => ("predicted_blocks", ["C00"]),
    ConfirmedBlocks => ("predicted_blocks_confirmed", ["C00"]),
    RevertedBlocks => ("predicted_blocks_reverted", ["C00","C10"]),
    RetiredPredictions => ("predictions_retired_on_boundary", ["C00","F05"]),
    ContainerRefused => ("container_input_locally_refused", ["E05","E06","E07"]),
    ContainerStale => ("container_input_stale", ["E05","E07"]),
    ContainerQueueFull => ("container_command_queue_full", ["E05"]),
    ContainerNotWritten => ("container_click_not_written", ["E05","E07"]),
    ContainerWritten => ("container_click_written_not_server_ack", ["E04","E06","E07"]),
    ContainerAuthorityRefused => ("container_update_admission_refused", ["E05","E07"]),
    ContainerViewRetired => ("container_pickup_view_retired", ["E05","E07"]),
    ActionQueueFull => ("action_command_queue_full", ["B03","B04","B05","C05","P08"]),
    CommandClosed => ("command_queue_closed", ["F05","F06"]),
    SwingWorldClear => ("local_swing_cleared_by_admitted_world_event", ["B03","B05","F05"]),
    UseStoppedReleased => ("active_use_stopped_while_input_released", ["B08","C05"]),
    UseStoppedDead => ("active_use_stopped_while_dead", ["B08","P11"]),
    UseStoppedOther => ("active_use_stopped_without_input_release", ["B08","C01","C02"]),
    SwingAccepted => ("local_swings_started", ["B03","B04","B05"]),
    SwingGuarded => ("swings_suppressed_by_native_half_duration_guard", ["B03","B05"]),
    SwingUnknown => ("swing_description_unavailable", ["B01","B03","B05"]),
    SwingNone => ("swings_with_explicit_none_animation", ["B03","B05"]),
    SwingAttempts => ("swing_commands_queued_not_hits", ["B03","B05","P08","P09"]),
    UseRequests => ("use_commands_queued_not_results", ["B04","B08","C09","P01","P04","P05","P06"]),
    UseReleases => ("use_release_commands_queued", ["B08","C05"]),
    UseStateStops => ("use_state_stopped_reason_unresolved", ["B08","C01"]),
    UseBoundaryCancellation => ("active_use_cancelled_on_world_boundary", ["B08","F05"]),
    AnimationHiddenFrames => ("active_animation_frames_intentionally_hidden", ["B03","B05","B08"]),
    AnimationUnavailableFrames => ("active_animation_frames_without_hand_publication", ["B03","B04","B05","B08"]),
    AnimationLostPublication => ("active_animation_lost_hand_publication", ["B03","B05","B08"]),
    EntityLocalCollision => ("remote_spawn_using_local_player_id", ["D10"]),
    EntitySpawnCollision => ("spawn_over_existing_entity_id", ["D10"]),
    EntityUnknownUpdates => ("entity_updates_without_current_identity", ["D10"]),
    EntityUnknownRemovals => ("despawns_without_current_identity", ["D10"]),
    StaleWorldEvents => ("world_events_not_admitted", ["F05","D10"]),
}
#[derive(Clone, Copy, Debug)]
pub enum Stage {
    FrameInterval,
    MainUpdate,
    TerrainPublication,
    EntitySkinPublication,
    HandTexturePublication,
}
impl Stage {
    const ALL: [Self; 5] = [
        Self::FrameInterval,
        Self::MainUpdate,
        Self::TerrainPublication,
        Self::EntitySkinPublication,
        Self::HandTexturePublication,
    ];
    fn key(self) -> &'static str {
        match self {
            Self::FrameInterval => "frame_interval",
            Self::MainUpdate => "main_schedule_wall_time",
            Self::TerrainPublication => "terrain_publication_cpu_scope",
            Self::EntitySkinPublication => "entity_skin_publication_cpu_scope",
            Self::HandTexturePublication => "hand_artwork_publication_cpu_scope",
        }
    }
    fn items(self) -> &'static [&'static str] {
        match self {
            Self::FrameInterval | Self::MainUpdate => &["I03", "D08", "A07"],
            Self::TerrainPublication => &["I01", "I03"],
            Self::EntitySkinPublication => &["D04", "D08", "I02", "I03"],
            Self::HandTexturePublication => &["B03", "E03", "I02", "I03"],
        }
    }
}
#[derive(Default)]
struct Timing {
    samples: u64,
    total_us: u64,
    max_us: u64,
    spikes: u64,
}
#[derive(Default)]
struct State {
    finished: bool,
    counts: std::collections::BTreeMap<&'static str, u64>,
    timings: std::collections::BTreeMap<&'static str, Timing>,
    recent: VecDeque<Value>,
    recent_spikes: VecDeque<Value>,
    recent_hand_issues: VecDeque<Value>,
    input_bits: Option<u8>,
    on_ground: Option<bool>,
    sprinting: Option<bool>,
    scope: Option<(u64, u64)>,
    using: bool,
    mining: bool,
    loading: [bool; 3],
    hand_published: bool,
    prediction_instrumented: bool,
    correction_total: f64,
    correction_max: f64,
    correction_samples: u64,
}
impl State {
    fn count(&mut self, c: Counter, amount: u64) {
        let n = self.counts.entry(c.key()).or_default();
        *n = n.saturating_add(amount);
    }
    fn spike_recent(&mut self, v: Value) {
        if self.recent_spikes.len() == RECENT {
            self.recent_spikes.pop_front();
        }
        self.recent_spikes.push_back(v);
    }
    fn recent(&mut self, v: Value) {
        if self.recent.len() == RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back(v);
    }
}
#[derive(Clone, Copy)]
pub enum Prediction {
    Applied,
    Confirmed,
    Reverted,
    Retired,
}
/// One recorder per native connection attempt; cloned handles never own gameplay state.
pub struct Recorder(Mutex<State>);
impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JavaTelemetry").finish_non_exhaustive()
    }
}
impl Recorder {
    pub fn configured(flag: Option<&str>) -> Option<Arc<Self>> {
        matches!(flag, Some("1" | "true")).then(|| Arc::new(Self(Mutex::new(State::default()))))
    }
    /// Files-app opt-in for iOS: exact bounded contents, read once per session.
    pub(super) fn from_directory(directory: &std::path::Path) -> Option<Arc<Self>> {
        use std::io::Read;
        let file = std::fs::File::open(directory.join("java-telemetry.enabled")).ok()?;
        let mut value = Vec::new();
        file.take(3).read_to_end(&mut value).ok()?;
        matches!(value.as_slice(), b"1" | b"1\n")
            .then(|| Self::configured(Some("1")))
            .flatten()
    }
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn count(&self, c: Counter, amount: u64) {
        if amount == 0 {
            return;
        }
        let mut s = self.state();
        if !s.finished {
            s.count(c, amount);
        }
    }
    /// Called only for successfully queued commands. It does not infer socket/server success.
    pub(super) fn command(&self, command: &Command) {
        let mut s = self.state();
        if s.finished {
            return;
        }
        let c = match command {
            Command::Move { frame, .. } => {
                if let Some(v) = frame.input {
                    s.input_bits = Some(v);
                }
                s.on_ground = Some(frame.on_ground);
                if let Some(v) = frame.sprinting {
                    s.sprinting = Some(v);
                }
                None
            }
            Command::Punch { .. } | Command::Attack { .. } => Some(Counter::SwingAttempts),
            Command::UseOn { .. } => {
                s.count(Counter::BlockRequests, 1);
                Some(Counter::UseRequests)
            }
            Command::UseItem { .. } | Command::UseEntity { .. } => Some(Counter::UseRequests),
            Command::Interact { action, .. } if action.action == Action::ReleaseUse => {
                Some(Counter::UseReleases)
            }
            Command::Interact { action, .. }
                if matches!(
                    action.action,
                    Action::Start | Action::Finish | Action::Abort | Action::ChangeDirection
                ) =>
            {
                match action.action {
                    Action::Start => s.mining = true,
                    Action::Abort | Action::Finish => s.mining = false,
                    _ => {}
                }
                Some(Counter::BlockRequests)
            }
            _ => None,
        };
        if let Some(c) = c {
            s.count(c, 1);
        }
    }
    /// Distance is calculated by the world owner after relative flags resolve, before simulation resumes.
    pub fn correction(&self, distance: Option<f64>, initial: bool, menu_open: bool) {
        let mut s = self.state();
        if s.finished {
            return;
        }
        let distance = distance.filter(|d| d.is_finite() && *d >= 0.);
        s.count(
            if initial {
                Counter::InitialPositions
            } else {
                Counter::Corrections
            },
            1,
        );
        if let Some(d) = distance {
            s.correction_total += d;
            s.correction_max = s.correction_max.max(d);
            s.correction_samples = s.correction_samples.saturating_add(1);
        }
        let event = json!({"kind":"resolved_server_position","distance_m":distance,"input_bits":s.input_bits,"on_ground":s.on_ground,"sprinting":s.sprinting,"menu_open":menu_open,"mining":s.mining,"using_item":s.using});
        s.recent(event);
    }
    pub fn action_state(&self, scope: (u64, u64), using: bool, mining: bool) {
        let mut s = self.state();
        if s.finished {
            return;
        }
        if s.scope != Some(scope) {
            if s.using {
                s.count(Counter::UseBoundaryCancellation, 1);
            }
            s.hand_published = false;
        } else if s.using && !using {
            s.count(Counter::UseStateStops, 1);
        }
        s.scope = Some(scope);
        s.using = using;
        s.mining = mining;
    }
    /// `hidden` means an intentional visibility/context gate, not an unexplained missing frame.
    pub fn hand_frame(&self, active: bool, published: bool, hidden: bool, cause: HandIssue) {
        let mut s = self.state();
        if s.finished {
            return;
        }
        if active && !published {
            s.count(
                if hidden {
                    Counter::AnimationHiddenFrames
                } else {
                    Counter::AnimationUnavailableFrames
                },
                1,
            );
            if s.hand_published && !hidden {
                s.count(Counter::AnimationLostPublication, 1);
                if s.recent_hand_issues.len() == RECENT {
                    s.recent_hand_issues.pop_front();
                }
                s.recent_hand_issues
                    .push_back(json!({"kind":"hand_publication_lost","cause":cause.key()}));
            }
        }
        s.hand_published = active && published;
    }
    /// Lane C must call this from actual prediction/reconciliation, never from ACK alone.
    pub fn prediction(&self, event: Prediction, amount: u64) {
        let c = match event {
            Prediction::Applied => Counter::PredictedBlocks,
            Prediction::Confirmed => Counter::ConfirmedBlocks,
            Prediction::Reverted => Counter::RevertedBlocks,
            Prediction::Retired => Counter::RetiredPredictions,
        };
        let mut s = self.state();
        if !s.finished {
            s.prediction_instrumented = true;
            s.count(c, amount);
        }
    }
    pub fn loading(&self, pending: [bool; 3]) {
        let mut s = self.state();
        if !s.finished {
            s.loading = pending;
        }
    }
    pub fn measure(&self, stage: Stage, elapsed: Duration) {
        let mut s = self.state();
        if s.finished {
            return;
        }
        let us = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        let v = s.timings.entry(stage.key()).or_default();
        v.samples = v.samples.saturating_add(1);
        v.total_us = v.total_us.saturating_add(us);
        v.max_us = v.max_us.max(us);
        if elapsed >= SPIKE {
            v.spikes = v.spikes.saturating_add(1);
            let activity = s.loading;
            s.spike_recent(json!({"kind":"timing_spike","stage":stage.key(),"elapsed_us":us,"pending_chunks_skins_textures":activity}));
        }
    }
    pub fn snapshot(&self) -> Value {
        self.document(false)
    }
    pub(super) fn finish(&self) -> Value {
        self.document(true)
    }
    fn document(&self, finish: bool) -> Value {
        let mut s = self.state();
        if finish && !s.finished {
            if s.using {
                s.count(Counter::UseBoundaryCancellation, 1);
            }
            s.finished = true;
        }
        let counts=Counter::ALL.iter().map(|c|(c.key().to_owned(),json!({"count":s.counts.get(c.key()).copied().unwrap_or(0),"checklist":c.items()}))).collect::<serde_json::Map<_,_>>();
        let times=Stage::ALL.iter().map(|stage| { let t=s.timings.get(stage.key());(stage.key().to_owned(),json!({"samples":t.map_or(0,|t|t.samples),"total_us":t.map_or(0,|t|t.total_us),"max_us":t.map_or(0,|t|t.max_us),"spikes":t.map_or(0,|t|t.spikes),"checklist":stage.items()})) }).collect::<serde_json::Map<_,_>>();
        json!({"schema":1,"enabled":true,"finished":s.finished,"scope":s.scope,"protocol":java_protocol::target::PROTOCOL,"game_version":java_protocol::target::GAME_VERSION,"counts":counts,"timings":times,"spike_threshold_ms":SPIKE.as_millis(),"correction_distance_m":{"samples":s.correction_samples,"total":s.correction_total,"max":s.correction_max},"recent":s.recent,"recent_spikes":s.recent_spikes,"recent_hand_issues":s.recent_hand_issues,"coverage":{"local_block_prediction":s.prediction_instrumented,"gpu_execution_timing":false,"server_transaction_acceptance_inferred":false},"interpretation":"Counters are observations, not automatic failures. Missing timing samples are unobserved, not passes. ACK/write receipts are not server acceptance."})
    }
}
/// CPU wall scope only; GPU submission/presentation time needs separate GPU evidence.
pub struct Scope {
    recorder: Arc<Recorder>,
    stage: Stage,
    start: web_time::Instant,
}
impl Scope {
    pub fn new(recorder: &Arc<Recorder>, stage: Stage) -> Self {
        Self {
            recorder: Arc::clone(recorder),
            stage,
            start: web_time::Instant::now(),
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        self.recorder.measure(self.stage, self.start.elapsed());
    }
}
#[cfg(test)]
mod tests;
