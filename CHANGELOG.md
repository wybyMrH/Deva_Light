# Changelog

## v0.1.28 - 2026-06-14

- 修复打开设置 / 远程连接时卡顿：诊断面板的日志预览从「读取整个日志文件」（日志会增长到数十 MB）改为只读尾部 16KB。
- 保存按钮在未做任何改动时显示为灰色（禁用），编辑后才变蓝可点击；显示模式 / 自动熄灭 / 自动更新这几个即时保存的选项不计入。

## v0.1.27 - 2026-06-14

- 修复频繁卡顿：项目识别（identify_project）改为缓存结果，不再为每个新会话重复跑 git 命令（WSL↔Windows 跨文件系统上 git 很慢，是卡顿主因）。
- 修复静默自动更新不生效：不再因「有活动灯」无限推迟，检测到新版并下载完成后立即静默重启（重启不影响 Cursor/Claude 进程，只重置监控灯）。
- 修复不开 TUN 模式时网络不走代理：启动时读取 Windows 系统代理并应用到所有网络请求（资讯面板 + 自动更新），无需手动设环境变量。

## v0.1.26 - 2026-06-14

- 新增「休息一下」资讯面板：右键灯打开悬浮面板，浏览 newsnow 资讯（知乎 / 微博 / V2EX / IT之家 / Hacker News 等 20 个源），分类浏览、收藏、自选展示哪些平台，点击标题用默认浏览器打开；数据源地址可在设置中改为自部署。
- Cursor 与 Claude Code 在同一项目时现显示为两个独立灯，hover 可见工具来源。
- 修复 Cursor 灯检测不到 / 不灵敏：启动时自动安装 Cursor 钩子，并新增近期 transcript 兜底发现。
- 修复新会话检测慢、点刷新无效：「刷新状态」现在主动扫描并立即点亮新会话。
- 修复 Cursor 跑 Bash 等命令时误判为黄灯，现正确显示为工作绿灯。

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
