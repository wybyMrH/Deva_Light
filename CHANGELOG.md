# Changelog

## v0.1.23 - 2026-06-08

- Add normalized agent event and provider capability foundations for Claude, Cursor, and Codex.
- Add pending waiting-action summaries so yellow lights can show why an agent is waiting without exposing sensitive payloads.
- Keep actionable approvals conservative: Codex and Cursor waiting states point users back to the provider instead of showing unsupported approve buttons.
- Improve error detection so auth, retry, connection, and HTTP 4xx/5xx failures stay as flashing red error lights instead of being treated as completed work.
- Surface error and pending-action details in the drawer, diagnostics, and tooltips.
- Add tests for error persistence, provider waiting summaries, and Codex waiting behavior.
