use crate::agent_event::{AgentEvent, AgentEventType, PendingActionSummary};
use crate::config::load_app_config;
use crate::monitor_origin::{compose_light_id, resolve_origin_display, resolve_origin_identity};
use crate::project::identify_project;
use crate::types::{LightState, SessionRef, Status, Tool};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

#[derive(Default)]
struct AggregatorState {
    lights: HashMap<String, LightState>,
    session_to_light: HashMap<String, String>,
    light_order: Vec<String>,
}

type ChangeCallback = Arc<dyn Fn() + Send + Sync>;
type SharedChangeCallback = Arc<RwLock<Option<ChangeCallback>>>;

#[derive(Clone, Default)]
pub struct StateAggregator {
    state: Arc<RwLock<AggregatorState>>,
    on_change: SharedChangeCallback,
}

#[derive(Debug, Clone)]
pub struct TrackedSession {
    pub session_id: String,
    pub tool: Tool,
    pub status: Status,
}

impl StateAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_session(&self, session_id: String, tool: Tool, cwd: &Path, status: Status) {
        self.add_session_with_context(session_id, tool, cwd, status, None);
    }

    pub fn add_session_with_context(
        &self,
        session_id: String,
        tool: Tool,
        cwd: &Path,
        status: Status,
        context_path: Option<&Path>,
    ) {
        let (logical_project_id, project_label) = identify_project(cwd);
        let identity = resolve_origin_identity(cwd, context_path);
        let origin = identity.origin;
        let light_id = compose_light_id(&logical_project_id, origin, tool);
        let workspace_path = cwd.to_string_lossy().to_string();

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");

            remove_existing_session(&mut state, &session_id);

            if !state.lights.contains_key(&light_id) {
                state.light_order.push(light_id.clone());
            }

            let light = state.lights.entry(light_id.clone()).or_insert_with(|| {
                LightState::new(
                    light_id.clone(),
                    logical_project_id.clone(),
                    project_label.clone(),
                    origin,
                    identity.key.clone(),
                    identity.detail.clone(),
                )
            });

            if light.workspace_path.is_none() {
                light.workspace_path = Some(workspace_path);
            }

            light.sessions.push(SessionRef {
                session_id: session_id.clone(),
                tool,
                status,
                started_at: Instant::now(),
                task_name: None,
                error_message: None,
                pending_action: None,
                monitor_origin: Some(origin),
                process_id: None,
            });
            light.last_event_at = Instant::now();
            light.aggregate_status();

            state.session_to_light.insert(session_id, light_id.clone());
        }

        self.notify_change();
    }

    pub fn workspace_path(&self, light_id: &str) -> Option<String> {
        let state = self.state.read().expect("aggregator state lock poisoned");
        state
            .lights
            .get(light_id)
            .and_then(|light| light.workspace_path.clone())
    }

    pub fn update_session_status(&self, session_id: &str, new_status: Status) {
        let mut changed = false;

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            let Some(light_id) = state.session_to_light.get(session_id).cloned() else {
                return;
            };

            if let Some(light) = state.lights.get_mut(&light_id) {
                if let Some(session) = light
                    .sessions
                    .iter_mut()
                    .find(|session| session.session_id == session_id)
                {
                    if session.status == Status::Error && new_status != Status::Error {
                        return;
                    }
                    session.status = new_status;
                    if new_status != Status::Error {
                        session.error_message = None;
                    }
                    if new_status != Status::Waiting {
                        session.pending_action = None;
                    }
                    light.last_event_at = Instant::now();
                    light.aggregate_status();
                    refresh_last_error(light);
                    changed = true;
                }
            }
        }

        if changed {
            self.notify_change();
        }
    }

    pub fn session_status(&self, session_id: &str) -> Option<Status> {
        let state = self.state.read().expect("aggregator state lock poisoned");
        let light_id = state.session_to_light.get(session_id)?;
        let light = state.lights.get(light_id)?;

        light
            .sessions
            .iter()
            .find(|session| session.session_id == session_id)
            .map(|session| session.status)
    }

    pub fn tracked_sessions(&self) -> Vec<TrackedSession> {
        let state = self.state.read().expect("aggregator state lock poisoned");

        state
            .lights
            .values()
            .flat_map(|light| {
                light.sessions.iter().map(|session| TrackedSession {
                    session_id: session.session_id.clone(),
                    tool: session.tool,
                    status: session.status,
                })
            })
            .collect()
    }

    pub fn remove_session(&self, session_id: &str) {
        let changed;

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            let Some(light_id) = state.session_to_light.remove(session_id) else {
                return;
            };

            let should_remove = if let Some(light) = state.lights.get_mut(&light_id) {
                light
                    .sessions
                    .retain(|session| session.session_id != session_id);
                light.last_event_at = Instant::now();

                if light.sessions.is_empty() {
                    true
                } else {
                    light.aggregate_status();
                    refresh_last_error(light);
                    false
                }
            } else {
                false
            };

            if should_remove {
                remove_light_by_id(&mut state, &light_id);
            }
            changed = true;
        }

        if changed {
            self.notify_change();
        }
    }

    pub fn apply_agent_event(&self, event: AgentEvent) {
        match event.event_type {
            AgentEventType::SessionStart => {
                let cwd = event
                    .cwd
                    .clone()
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_else(|| Path::new(".").to_path_buf());
                self.add_session(
                    event.session_id.clone(),
                    event.provider.tool(),
                    &cwd,
                    Status::Idle,
                );
            }
            AgentEventType::SessionEnd => {
                if self.session_status(&event.session_id) == Some(Status::Error) {
                    return;
                }
                self.remove_session(&event.session_id);
                return;
            }
            _ => {}
        }

        if let Some(status) = event.status {
            self.update_session_status(&event.session_id, status);
            if status == Status::Error {
                if let Some(message) = event.summary.clone().or_else(|| event.task_hint.clone()) {
                    self.set_error_message(&event.session_id, message);
                }
            }
        }

        if let Some(task_hint) = event.task_hint.filter(|value| !value.is_empty()) {
            self.set_task_name(&event.session_id, task_hint);
        }

        if let Some(tool_name) = event.tool_name.filter(|value| !value.is_empty()) {
            self.set_last_tool_call(&event.session_id, tool_name);
        }

        if let Some(pending_action) = event.pending_action {
            self.set_pending_action(&event.session_id, pending_action);
        }
    }

    pub fn confirm_light(&self, project_id: &str) {
        let mut changed = false;

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            let Some(status) = state.lights.get(project_id).map(|light| light.status) else {
                return;
            };

            match status {
                Status::Done => {
                    remove_light_by_id(&mut state, project_id);
                    changed = true;
                }
                Status::Error => {
                    let error_session_ids = state
                        .lights
                        .get(project_id)
                        .map(|light| {
                            light
                                .sessions
                                .iter()
                                .filter(|session| session.status == Status::Error)
                                .map(|session| session.session_id.clone())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    if error_session_ids.is_empty() {
                        return;
                    }

                    for session_id in &error_session_ids {
                        state.session_to_light.remove(session_id);
                    }

                    let should_remove = if let Some(light) = state.lights.get_mut(project_id) {
                        light
                            .sessions
                            .retain(|session| session.status != Status::Error);
                        light.last_event_at = Instant::now();
                        if light.sessions.is_empty() {
                            true
                        } else {
                            light.aggregate_status();
                            refresh_last_error(light);
                            false
                        }
                    } else {
                        false
                    };

                    if should_remove {
                        remove_light_by_id(&mut state, project_id);
                    }
                    changed = true;
                }
                Status::Waiting => {
                    let Some(has_no_sessions) = state
                        .lights
                        .get(project_id)
                        .map(|light| light.sessions.is_empty())
                    else {
                        return;
                    };

                    if has_no_sessions {
                        remove_light_by_id(&mut state, project_id);
                        changed = true;
                    } else {
                        let Some(light) = state.lights.get_mut(project_id) else {
                            return;
                        };

                        for session in &mut light.sessions {
                            if session.status == Status::Waiting {
                                session.status = Status::Idle;
                                session.error_message = None;
                                session.pending_action = None;
                                changed = true;
                            }
                        }
                        if changed {
                            light.last_event_at = Instant::now();
                            light.aggregate_status();
                            refresh_last_error(light);
                        }
                    }
                }
                Status::Idle | Status::Working => {}
            }
        }

        if changed {
            self.notify_change();
        }
    }

    pub fn remove_light(&self, project_id: &str) {
        let removed = {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            remove_light_by_id(&mut state, project_id)
        };

        if removed {
            self.notify_change();
        }
    }

    pub fn confirm_session(&self, session_id: &str) {
        let mut changed = false;
        let mut remove_tracking = false;

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            let Some(light_id) = state.session_to_light.get(session_id).cloned() else {
                return;
            };

            let Some(light) = state.lights.get_mut(&light_id) else {
                return;
            };

            if let Some(session) = light
                .sessions
                .iter_mut()
                .find(|s| s.session_id == session_id)
            {
                match session.status {
                    Status::Done | Status::Error => {
                        light.sessions.retain(|s| s.session_id != session_id);
                        changed = true;
                        remove_tracking = true;
                    }
                    Status::Waiting => {
                        session.status = Status::Idle;
                        session.error_message = None;
                        session.pending_action = None;
                        light.last_event_at = Instant::now();
                        light.aggregate_status();
                        refresh_last_error(light);
                        changed = true;
                    }
                    Status::Idle | Status::Working => {}
                }
            }

            if changed && remove_tracking {
                state.session_to_light.remove(session_id);
                let should_remove = state
                    .lights
                    .get(&light_id)
                    .is_some_and(|light| light.sessions.is_empty());

                if should_remove {
                    remove_light_by_id(&mut state, &light_id);
                } else if let Some(light) = state.lights.get_mut(&light_id) {
                    light.last_event_at = Instant::now();
                    light.aggregate_status();
                    refresh_last_error(light);
                }
            }
        }

        if changed {
            self.notify_change();
        }
    }

    pub fn get_lights(&self) -> Vec<LightState> {
        let state = self.state.read().expect("aggregator state lock poisoned");
        let aliases = load_app_config().origin_aliases;

        state
            .light_order
            .iter()
            .filter_map(|light_id| state.lights.get(light_id).cloned())
            .filter(|light| light.is_active())
            .map(|mut light| {
                light.origin_display = Some(resolve_origin_display(
                    &crate::monitor_origin::OriginIdentity {
                        origin: light.monitor_origin,
                        key: light.origin_key.clone(),
                        detail: light.origin_detail.clone(),
                    },
                    &aliases,
                ));
                light
            })
            .collect()
    }

    pub fn has_active_lights(&self) -> bool {
        let state = self.state.read().expect("aggregator state lock poisoned");
        state.lights.values().any(|light| light.is_active())
    }

    pub fn prune_expired_done_lights(&self, retention: Duration) -> bool {
        let mut removed = false;

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            let now = Instant::now();
            let expired_light_ids: Vec<String> = state
                .lights
                .iter()
                .filter(|(_, light)| {
                    !light.sessions.is_empty()
                        && light
                            .sessions
                            .iter()
                            .all(|session| session.status == Status::Done)
                        && now.saturating_duration_since(light.last_event_at) >= retention
                })
                .map(|(light_id, _)| light_id.clone())
                .collect();

            for light_id in expired_light_ids {
                removed |= remove_light_by_id(&mut state, &light_id);
            }
        }

        if removed {
            self.notify_change();
        }

        removed
    }

    pub fn set_task_name(&self, session_id: &str, task_name: String) {
        let display = summarize_task_name(task_name);
        let mut changed = false;

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            let Some(light_id) = state.session_to_light.get(session_id).cloned() else {
                return;
            };

            if let Some(light) = state.lights.get_mut(&light_id) {
                if let Some(session) = light
                    .sessions
                    .iter_mut()
                    .find(|session| session.session_id == session_id)
                {
                    session.task_name = Some(display);
                    light.last_event_at = Instant::now();
                    changed = true;
                }
            }
        }

        if changed {
            self.notify_change();
        }
    }

    pub fn set_error_message(&self, session_id: &str, message: String) {
        let display = summarize_error_message(message);
        let mut changed = false;

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            let Some(light_id) = state.session_to_light.get(session_id).cloned() else {
                return;
            };

            if let Some(light) = state.lights.get_mut(&light_id) {
                if let Some(session) = light
                    .sessions
                    .iter_mut()
                    .find(|session| session.session_id == session_id)
                {
                    session.status = Status::Error;
                    session.error_message = Some(display.clone());
                    session.pending_action = None;
                    light.last_error = Some(display);
                    light.last_event_at = Instant::now();
                    light.aggregate_status();
                    changed = true;
                }
            }
        }

        if changed {
            self.notify_change();
        }
    }

    pub fn set_last_tool_call(&self, session_id: &str, tool_call: String) {
        let mut changed = false;

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            let Some(light_id) = state.session_to_light.get(session_id).cloned() else {
                return;
            };

            if let Some(light) = state.lights.get_mut(&light_id) {
                light.last_tool_call = Some(tool_call);
                light.last_event_at = Instant::now();
                changed = true;
            }
        }

        if changed {
            self.notify_change();
        }
    }

    pub fn set_pending_action(&self, session_id: &str, pending_action: PendingActionSummary) {
        let mut changed = false;

        {
            let mut state = self.state.write().expect("aggregator state lock poisoned");
            let Some(light_id) = state.session_to_light.get(session_id).cloned() else {
                return;
            };

            if let Some(light) = state.lights.get_mut(&light_id) {
                if let Some(session) = light
                    .sessions
                    .iter_mut()
                    .find(|session| session.session_id == session_id)
                {
                    session.pending_action = Some(pending_action);
                    light.last_event_at = Instant::now();
                    changed = true;
                }
            }
        }

        if changed {
            self.notify_change();
        }
    }

    pub fn set_on_change<F>(&self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut on_change = self
            .on_change
            .write()
            .expect("aggregator callback lock poisoned");
        *on_change = Some(Arc::new(callback));
    }

    fn notify_change(&self) {
        let callback = self
            .on_change
            .read()
            .expect("aggregator callback lock poisoned")
            .clone();

        if let Some(callback) = callback {
            callback();
        }
    }
}

fn remove_existing_session(state: &mut AggregatorState, session_id: &str) {
    let Some(light_id) = state.session_to_light.remove(session_id) else {
        return;
    };

    let should_remove = if let Some(light) = state.lights.get_mut(&light_id) {
        light
            .sessions
            .retain(|session| session.session_id != session_id);

        if light.sessions.is_empty() {
            true
        } else {
            light.aggregate_status();
            refresh_last_error(light);
            false
        }
    } else {
        false
    };

    if should_remove {
        remove_light_by_id(state, &light_id);
    }
}

fn summarize_task_name(task_name: String) -> String {
    let collapsed = task_name.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_LEN: usize = 48;

    if collapsed.chars().count() <= MAX_LEN {
        collapsed
    } else {
        let truncated: String = collapsed.chars().take(MAX_LEN.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

fn summarize_error_message(message: String) -> String {
    let collapsed = message.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_LEN: usize = 140;

    if collapsed.chars().count() <= MAX_LEN {
        collapsed
    } else {
        let truncated: String = collapsed.chars().take(MAX_LEN.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

fn refresh_last_error(light: &mut LightState) {
    light.last_error = light
        .sessions
        .iter()
        .find(|session| session.status == Status::Error)
        .and_then(|session| session.error_message.clone());
}

fn remove_light_by_id(state: &mut AggregatorState, light_id: &str) -> bool {
    let mut removed = false;

    if let Some(light) = state.lights.remove(light_id) {
        for session in light.sessions {
            state.session_to_light.remove(&session.session_id);
        }
        removed = true;
    }

    state
        .light_order
        .retain(|existing_light_id| existing_light_id != light_id);

    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor_origin::MonitorOrigin;
    use crate::types::Tool;

    #[test]
    fn hides_lights_without_active_sessions() {
        let agg = StateAggregator::new();

        agg.add_session(
            "idle-session".to_string(),
            Tool::ClaudeCode,
            Path::new(r"C:\demo"),
            Status::Idle,
        );
        assert!(agg.get_lights().is_empty());

        agg.update_session_status("idle-session", Status::Working);
        assert_eq!(agg.get_lights().len(), 1);

        agg.update_session_status("idle-session", Status::Done);
        let lights = agg.get_lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].status, Status::Done);

        assert!(agg.prune_expired_done_lights(Duration::ZERO));
        assert!(agg.get_lights().is_empty());
    }

    #[test]
    fn splits_same_logical_project_by_origin() {
        let agg = StateAggregator::new();

        agg.add_session(
            "local".to_string(),
            Tool::ClaudeCode,
            Path::new(r"C:\Users\alice\projects\demo"),
            Status::Working,
        );
        agg.add_session(
            "remote".to_string(),
            Tool::ClaudeCode,
            Path::new("/home/user/demo"),
            Status::Waiting,
        );

        let lights = agg.get_lights();
        assert_eq!(lights.len(), 2);
        assert!(lights
            .iter()
            .any(|light| light.monitor_origin == MonitorOrigin::Local));
        #[cfg(target_os = "windows")]
        assert!(lights
            .iter()
            .any(|light| light.monitor_origin == MonitorOrigin::Remote));
    }
}
