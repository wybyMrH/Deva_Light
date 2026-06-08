use super::ProviderCapabilities;

pub fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        live_status: true,
        session_restore: true,
        blocking_decision: false,
        approve: false,
        deny: false,
        ask: false,
        defer: true,
        updated_input: false,
        usage: false,
        file_diff: false,
        remote_decision: false,
    }
}
