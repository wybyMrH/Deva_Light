use crate::agent_event::{
    PendingActionKind, PrivacyLevel, ProviderId, TimeoutDecision, UserDecisionKind,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderActionRef {
    pub raw_event_type: String,
    pub event_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingAction {
    pub action_id: String,
    pub provider: ProviderId,
    pub session_id: String,
    pub kind: PendingActionKind,
    pub title: String,
    pub summary: Option<String>,
    pub tool_name: Option<String>,
    pub command_preview: Option<String>,
    pub file_paths: Vec<String>,
    pub decisions: Vec<UserDecisionKind>,
    pub default_on_timeout: TimeoutDecision,
    pub expires_at_ms: i64,
    pub privacy: PrivacyLevel,
    pub provider_ref: ProviderActionRef,
}

#[derive(Clone, Default)]
pub struct PendingActionStore {
    actions: Arc<RwLock<HashMap<String, PendingAction>>>,
}

impl PendingActionStore {
    pub fn upsert(&self, action: PendingAction) {
        self.actions
            .write()
            .expect("pending action store lock poisoned")
            .insert(action.action_id.clone(), action);
    }

    pub fn get(&self, action_id: &str) -> Option<PendingAction> {
        self.actions
            .read()
            .expect("pending action store lock poisoned")
            .get(action_id)
            .cloned()
    }

    pub fn remove(&self, action_id: &str) -> Option<PendingAction> {
        self.actions
            .write()
            .expect("pending action store lock poisoned")
            .remove(action_id)
    }

    pub fn prune_expired(&self, now_ms: i64) -> usize {
        let mut actions = self
            .actions
            .write()
            .expect("pending action store lock poisoned");
        let before = actions.len();
        actions.retain(|_, action| action.expires_at_ms > now_ms);
        before.saturating_sub(actions.len())
    }
}
