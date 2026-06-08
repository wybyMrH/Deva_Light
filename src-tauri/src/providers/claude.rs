use super::ProviderCapabilities;

pub fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        live_status: true,
        session_restore: false,
        blocking_decision: true,
        approve: true,
        deny: true,
        ask: true,
        defer: true,
        updated_input: true,
        usage: false,
        file_diff: false,
        remote_decision: false,
    }
}
