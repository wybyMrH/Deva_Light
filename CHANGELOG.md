# Changelog

## v0.1.25 - 2026-06-13

- 新增静默自动更新：空闲时后台下载新版本并自动安装重启，有 AI 任务进行时推迟；可在「设置 → 关于」关闭。
- 「显示模式」「完成红灯自动收起」改为勾选即保存，不再需要额外点击保存按钮。
- 修复启动 / 保存 / 刷新时弹出 PowerShell 窗口并卡顿：本机模式不再探测局域网 IP，仅局域网转发模式按需探测并在启动时后台预热。

## v0.1.23 - 2026-06-08

- Add normalized agent event and provider capability foundations for Claude, Cursor, and Codex.
- Add pending waiting-action summaries so yellow lights can show why an agent is waiting without exposing sensitive payloads.
- Keep actionable approvals conservative: Codex and Cursor waiting states point users back to the provider instead of showing unsupported approve buttons.
- Improve error detection so auth, retry, connection, and HTTP 4xx/5xx failures stay as flashing red error lights instead of being treated as completed work.
- Surface error and pending-action details in the drawer, diagnostics, and tooltips.
- Add tests for error persistence, provider waiting summaries, and Codex waiting behavior.
