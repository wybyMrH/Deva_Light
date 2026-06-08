use crate::agent_event::ProviderId;
use serde::Serialize;

pub mod claude;
pub mod codex;
pub mod cursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub live_status: bool,
    pub session_restore: bool,
    pub blocking_decision: bool,
    pub approve: bool,
    pub deny: bool,
    pub ask: bool,
    pub defer: bool,
    pub updated_input: bool,
    pub usage: bool,
    pub file_diff: bool,
    pub remote_decision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilityView {
    pub provider: ProviderId,
    pub capabilities: ProviderCapabilities,
}

pub fn provider_from_source(source: Option<&str>) -> ProviderId {
    match source {
        Some("cursor") => ProviderId::Cursor,
        _ => ProviderId::ClaudeCode,
    }
}

pub fn capabilities(provider: ProviderId) -> ProviderCapabilities {
    match provider {
        ProviderId::ClaudeCode => claude::capabilities(),
        ProviderId::Cursor => cursor::capabilities(),
        ProviderId::Codex => codex::capabilities(),
    }
}

pub fn all_provider_capabilities() -> Vec<ProviderCapabilityView> {
    [
        ProviderId::ClaudeCode,
        ProviderId::Cursor,
        ProviderId::Codex,
    ]
    .into_iter()
    .map(|provider| ProviderCapabilityView {
        provider,
        capabilities: capabilities(provider),
    })
    .collect()
}
