# Open Source Ideas Research - 2026-06-08

This note compares Deva Light with recent open-source projects around AI coding-agent monitoring, status display, hooks, and usage analytics. It is based on the current Deva Light codebase plus GitHub search results checked on 2026-06-08.

## Current Deva Light Baseline

Deva Light already has a clear niche:

- Floating desktop traffic-light UI for Claude Code, Cursor, and Codex.
- Project + source aggregation across local, WSL, SSH, and remote LAN origins.
- Hook-based Claude/Cursor events plus Codex session-file watching.
- Session drawer, parallel/compact display modes, notifications, diagnostics, SSH targets, and updater.

The current data model is intentionally small: `SessionRef` stores session id, tool, status, task name, origin, and process id; `LightState` groups sessions by project/source and exposes status, path, last tool call, and workspace metadata.

## Source Notes

GitHub's unauthenticated repository detail API was rate-limited during this research, so repo descriptions, stars, and update recency below come from GitHub repository search results rather than per-repo API calls. Links are included so each project can be rechecked later.

## Closest Projects

### AgentGlance

URL: https://github.com/hezi/AgentGlance

Search result summary: macOS overlay for monitoring AI coding agents such as Claude Code, Codex, and Gemini CLI in real time.

Good ideas:

- Floating overlay close to Deva Light's product shape.
- Multi-agent awareness rather than Claude-only monitoring.
- A richer per-session surface can complement the small always-on lamp.

Fit for Deva Light: high. Deva Light already owns the lightweight overlay position; the best borrow is a more actionable drawer, not a heavier main window.

### Cogpit

URL: https://github.com/gentritbiba/cogpit

Search result summary: desktop/browser dashboard for browsing, inspecting, and chatting with Claude Code agent sessions.

Good ideas:

- Session inspection as a separate deeper view.
- Browser and desktop surfaces over the same live session data.
- Conversation/session browsing for after-the-fact review.

Fit for Deva Light: medium. A full session browser is heavier than Deva Light's overlay, but an optional secondary dashboard would fit after a local event store exists.

### ccboard

URL: https://github.com/FlorianBruniaux/ccboard

Search result summary: Rust single-binary monitor for Claude Code sessions, costs, config, hooks, agents, MCP servers, TUI/web UI, process tracking, budget alerts, and forecasting.

Good ideas:

- Single Rust binary with live process tracking.
- TUI + web interface over the same backend.
- Config/hooks/agents/MCP/session/cost data in one dashboard.
- Budget alerts and forecasting.

Fit for Deva Light: high for architecture, medium for UI. Deva Light can borrow the provider/process/cost model while keeping the primary UI as a small traffic light.

### Claude Code Hooks Multi-Agent Observability

URL: https://github.com/disler/claude-code-hooks-multi-agent-observability

Search result summary: real-time monitoring for Claude Code agents through hook event tracking.

Good ideas:

- Hook-first event capture.
- Multi-agent observability from normalized hook events.
- Real-time dashboard pattern.

Fit for Deva Light: high. Deva Light already has hooks; the next step is a normalized `AgentEvent` layer instead of directly translating every signal into only `Status`.

### Claude Code Agent Monitor

URL: https://github.com/hoangsonww/Claude-Code-Agent-Monitor

Search result summary: SQLite, Node/Express, React, Vite, Tailwind, and WebSockets dashboard tracking sessions, agent activity, tool usage, sub-agent orchestration, analytics, Kanban board, notifications, and macOS native app.

Good ideas:

- SQLite-backed monitoring.
- WebSocket live updates.
- Tool usage and sub-agent tracking.
- Kanban/status-board view for many concurrent tasks.

Fit for Deva Light: medium. The stack is different, but local persistence and tool/sub-agent fields are directly relevant.

### Claude Watch

URL: https://github.com/blackwell-systems/claudewatch

Search result summary: AgentOps-style monitoring for Claude Code, including error loops, drift detection, friction analysis, cost-per-commit, success rates, and exportable metrics.

Good ideas:

- Detect stuck/error loops rather than just showing "working."
- Track friction and task success signals.
- Connect agent work to commits.

Fit for Deva Light: medium. Deva Light can add light heuristics such as "working too long", repeated command failures, or high-risk waiting state without becoming an AgentOps suite.

## Usage And Quota Projects

### Claude Usage

URL: https://github.com/phuryn/claude-usage

Search result summary: local dashboard for Claude Code token usage, costs, session history, and Pro/Max progress bar.

Good ideas:

- Local usage dashboard.
- Session history tied to token/cost data.
- Subscription/window progress indicators.

Fit for Deva Light: high for optional quota glance. A small "quota pressure" indicator is valuable, but it should not clutter the lamp.

### Sniffly

URL: https://github.com/chiphuyen/sniffly

Search result summary: Claude Code dashboard with usage stats, error analysis, and sharing.

Good ideas:

- Error analysis on top of session data.
- Shareable session or usage summaries.
- Human-readable analytics instead of raw logs.

Fit for Deva Light: medium. Error analysis is useful if Deva Light adds an event store; sharing should stay opt-in because this app handles local development metadata.

### TokenTracker

URL: https://github.com/mm7894215/TokenTracker

Search result summary: local-first token usage across many AI coding tools including Claude Code, Codex, Cursor, Gemini, Roo Code, Zed Agent, and Goose, with dashboard, macOS menu bar app, and widgets.

Good ideas:

- Broad provider coverage.
- Local-first zero-config approach.
- Native menu bar and desktop widget surfaces.

Fit for Deva Light: high. This reinforces the need for a provider boundary so Deva Light can add tools without copy-pasting watcher logic.

### Token Dashboard

URL: https://github.com/nateherkai/token-dashboard

Search result summary: turns raw JSONL transcripts into local cost analytics, hotspot views, and session-level usage insight.

Good ideas:

- JSONL transcript parsing.
- Hotspot views for where tokens are burned.
- Session-level analytics without cloud upload.

Fit for Deva Light: medium. Useful for a secondary "usage details" panel, not the main overlay.

### AI Token Monitor

URL: https://github.com/amadormateo/ai-token-monitor

Search result summary: tracks AI token usage, cost, and activity in real time for Claude Code, Codex, and OpenCode on macOS and Windows.

Good ideas:

- Real-time token and activity tracking.
- Cross-platform desktop coverage.
- Claude/Codex/OpenCode provider mix.

Fit for Deva Light: high. This overlaps strongly with the natural next step after status monitoring.

## Statusline And Glanceable UI Projects

### Claude HUD

URL: https://github.com/jarrodwatts/claude-hud

Search result summary: Claude Code plugin showing context usage, active tools, running agents, and todo progress.

Good ideas:

- Active tool display.
- Running agent count.
- Todo/progress awareness.
- Context usage as a glanceable status element.

Fit for Deva Light: high. Deva Light should surface active tool and todo/progress context in the drawer, while the lamp remains just status.

### ccstatusline

URL: https://github.com/sirmalloc/ccstatusline

Search result summary: customizable Claude Code CLI statusline with themes and powerline support.

Good ideas:

- User-configurable density and styling.
- Terminal-native compactness.
- Theme presets.

Fit for Deva Light: medium. Visual customization can help, but Deva Light should avoid turning settings into theme overload.

### Claude Code Usage Bar

URL: https://github.com/leeguooooo/claude-code-usage-bar

Search result summary: lightweight status line for 5h/7d rate-limit usage, reset countdowns, model, context window, prompt-cache age, and fast daemon mode.

Good ideas:

- Reset countdowns.
- Model/context/prompt-cache age in one compact view.
- Daemon fast-mode for low-latency status.

Fit for Deva Light: high for quota and reset-window display.

## Experimental Cross-Agent Observability

### AgentPulse

URL: https://github.com/Conalh/AgentPulse

Search result summary: local deterministic TUI dashboard watching Claude Code, Cursor, and Codex transcripts to show what each agent is doing.

Good ideas:

- Multi-tool transcript watching.
- Deterministic local classification of "what the agent is doing."
- No-LLM approach for privacy and predictability.

Fit for Deva Light: high conceptually. A deterministic "activity summary" is a better fit than sending session contents to another model.

### Argus

URL: https://github.com/kr4t0n/argus

Search result summary: multi-machine dashboard for CLI coding agents with Go sidecars, Redis Streams, React/Socket.IO frontend, token streaming, file diffs, and opt-in PTY shells.

Good ideas:

- Sidecar per machine.
- Multi-machine control plane.
- Token-level streaming and file diff surfaces.
- Opt-in PTY shell access.

Fit for Deva Light: low to medium. Deva Light already supports SSH/remote origins, but should keep remote control conservative and avoid PTY access unless there is a separate security design.

## Product Direction

Do not compete with full dashboards as the first move. Deva Light should stay the "at-a-glance attention layer" and add one deeper layer only when the user asks for it.

Recommended positioning:

> Deva Light is the lightweight cross-agent attention overlay. It tells you what needs your eyes now, then gives just enough context and action to handle it without switching windows.

## Upgrade Roadmap

### 1. Rich Session Drawer

Borrow from AgentGlance, Cogpit, Claude HUD, AgentPulse, and the usage dashboards.

Add fields to `SessionRef` or a companion `SessionDetail`:

- `started_at_epoch`, `updated_at_epoch`, `elapsed_seconds`
- `initial_prompt`
- `model`
- `token_input`, `token_output`, `estimated_cost`
- `progress_summary`
- `last_tool_name`, `last_tool_target`
- `waiting_reason`
- `branch_name`, `worktree_path`

UI changes:

- Show elapsed time and original prompt in each session row.
- Add a compact event trail: last 3 tool calls, waiting reason, last output snippet if available.
- Add filters: Waiting / Working / Done, tool, source, project.
- Add "jump to terminal" later, only where terminal detection is reliable.

Why first: it fits the existing drawer and does not require changing the main traffic-light metaphor.

### 2. Normalize Events Before Adding More Watchers

Borrow from hook observability projects and ccboard.

Current watchers mostly collapse input into `Status`. Add a normalized `AgentEvent` layer:

```rust
pub struct AgentEvent {
    pub event_id: String,
    pub session_id: String,
    pub tool: Tool,
    pub project_id: String,
    pub origin: MonitorOrigin,
    pub event_type: AgentEventType,
    pub status: Option<Status>,
    pub timestamp_ms: i64,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub tool_name: Option<String>,
    pub file_paths: Vec<String>,
    pub usage: Option<UsageSample>,
    pub privacy_level: PrivacyLevel,
}
```

Benefits:

- Richer drawer without special-casing every provider.
- History, usage, waiting reasons, and diagnostics can share one pipeline.
- New tools can be added with less UI churn.

### 3. Local Event Store

Borrow from Claude Code Agent Monitor, Token Dashboard, Claude Usage, and Sniffly.

Add a local SQLite store under `~/.deva_light`:

- `sessions`
- `events`
- `tool_calls`
- `usage_samples`
- `projects`
- `origins`

Benefits:

- Survives app restarts.
- Enables history search and post-mortem review.
- Makes stale-session pruning deterministic.
- Provides data for cost/quota and trend features.

Keep retention configurable: 7 days / 30 days / forever / disabled.

### 4. Actionable Waiting State

Current yellow means "go find the terminal." Upgrade yellow to "handle here when safe":

- Parse permission/question events into a normalized `WaitingReason`.
- Show approve/deny/skip actions only for integrations that support a safe response path.
- For edit approvals, show a concise file list first; inline diff can be a second phase.
- Preserve a strong privacy boundary: no file contents in remote mode unless enabled.

This should come after the event layer, because approvals need richer data than status alone.

Detailed design: [ACTIONABLE_WAITING_PROVIDER_BOUNDARY.md](ACTIONABLE_WAITING_PROVIDER_BOUNDARY.md).

### 5. Quota And Cost Glance

Borrow from Claude Usage, TokenTracker, Claude Code Usage Bar, AI Token Monitor, and Token Dashboard.

Add optional usage indicators:

- Per-tool quota pressure: green/orange/red ring or tiny text badge.
- 5-hour/weekly window estimates for Claude Code where available.
- Codex token/model breakdown if session files expose it.
- Reset countdowns.
- Cost estimates clearly marked as estimates.
- Notify at configurable thresholds, such as 80% and 90%.

Important: usage should be optional and secondary; main lamps should not become cluttered.

### 6. Provider Boundary

Borrow from TokenTracker, ccboard, AI Token Monitor, and AgentPulse.

Currently each tool has its own watcher. Formalize a provider interface:

- `detect()`
- `watch()`
- `parse_session()`
- `parse_usage()`
- `install_hooks()`
- `capabilities()`

Then new providers can be added without touching UI logic:

- Gemini CLI
- OpenCode
- GitHub Copilot CLI/Agent
- Goose
- Roo Code

This also makes test fixtures easier.

Detailed design: [ACTIONABLE_WAITING_PROVIDER_BOUNDARY.md](ACTIONABLE_WAITING_PROVIDER_BOUNDARY.md).

### 7. Remote Web Companion

Borrow from Cogpit, Argus, and hook observability dashboards.

Deva Light already has a local HTTP server and token. A small web companion could expose:

- Active sessions list.
- Waiting approvals.
- Read-only status from phone.
- Optional LAN QR code in settings.
- SSE `/events` endpoint for live state.

Security baseline:

- Token required by default.
- Local-only default bind remains `127.0.0.1`.
- Redacted mode for remote sessions.
- Clear "LAN enabled" warning and one-click token rotation.

### 8. Project Context Layer

Add enough project context to answer "is this task really done?":

- current branch
- worktree status
- dirty file count
- PR/MR link if discoverable
- CI status if GitHub/Gitea token is configured
- repeated failure or stuck-working heuristic

This gives Deva Light a useful "task health" signal without becoming a prompt-to-PR orchestrator.

## Suggested Priority

1. Rich session drawer with elapsed time, prompt, last tool, waiting reason, and better grouping.
2. Normalize hook/session events into an `AgentEvent` layer.
3. Add local SQLite history with configurable retention.
4. Add actionable waiting approvals for events that can be safely handled.
5. Add quota/cost glance as an optional secondary panel.
6. Add provider interface and one new provider, preferably Gemini CLI or OpenCode.
7. Add phone-friendly web companion over the existing HTTP server.
8. Add project branch/worktree/PR context.

## What To Avoid

- Do not put full conversation timelines in the main floating lamp UI.
- Do not make remote/LAN features send file contents by default.
- Do not make cost estimates look exact unless backed by provider data.
- Do not build a full prompt-to-PR orchestrator inside Deva Light yet; that is a separate product class.
- Do not add many providers through copy-pasted watcher logic. Add a provider boundary first.

## First Implementation Slice

A practical first PR:

- Extend session metadata with `updated_at`, `elapsed_seconds`, `initial_prompt`, `last_tool_call`, and `waiting_reason`.
- Update the drawer to show these fields.
- Add fixture-based tests for aggregation and stale-session behavior.
- Keep all new fields optional so existing Claude/Cursor/Codex paths continue working.

This makes the app immediately more useful while preparing for approvals, history, and usage analytics.
