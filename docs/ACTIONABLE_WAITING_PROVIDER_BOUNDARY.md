# 可操作黄灯与 Provider 边界设计

日期：2026-06-08

目标：把 Deva Light 的黄灯从“提醒你去终端看一眼”，升级为“在安全且 provider 支持时，可直接处理等待项”，同时把 Claude Code / Cursor / Codex 的接入逻辑从 UI 和聚合器里拆出来，便于后续增加 Gemini CLI、OpenCode、Goose 等工具。

## 现状判断

当前链路很直接：

```text
src-hook / watcher
  -> HTTP /events 或 JSONL poll
  -> HookEvent / CodexLineEvent
  -> Status
  -> StateAggregator
  -> LightState / SessionRef
  -> app.js / drawer.js
```

问题是：

- 大多数输入在进入 `StateAggregator` 前就被压缩成 `Status`，黄灯只知道 `Waiting`，不知道“为什么等”和“能不能处理”。
- `confirm_session` 现在语义是“我知道了/清除等待”，不是 approve/deny，不能复用成权限决策。
- `src-hook` 现在是 fire-and-forget：读取 stdin，POST `/events`，收到 200 就退出。它没有等待 GUI 决策，也不会把决策 JSON 写回原 provider。
- `hook_installer.rs` 同时知道 Claude 和 Cursor 的安装细节；`http_server.rs` 同时知道事件解析、provider 判断、状态迁移规则。后续加 provider 会越来越粘。

## 设计原则

- `Status::Waiting` 只是视觉状态，不等于一定可操作。
- approve/deny 必须由 provider capability 显式声明，UI 不能凭事件名猜。
- 先支持可靠 provider，再做通用外观。第一期优先 Claude Code；Cursor 先做 deny/展示，Codex 先做跳转/提示。
- 决策必须能回到原始阻塞中的 hook 进程，否则 UI 上的“批准”只是假的。
- 远程/LAN 模式默认只传元数据，不传文件内容和完整命令输出。

## Provider 边界

### 新增模块建议

```text
src-tauri/src/
  agent_event.rs          # 标准事件、等待动作、决策类型
  pending_action.rs       # 等待动作存储、超时、唤醒
  providers/
    mod.rs                # provider trait / registry / capabilities
    claude.rs             # Claude hook normalizer + decision renderer
    cursor.rs             # Cursor hook normalizer + conservative capabilities
    codex.rs              # Codex rollout normalizer
```

先不要做外部插件系统。第一步只做内部 provider adapter，避免大重构。

### ProviderId

保留现有 `Tool` 作为 UI 上的工具标签，但新增 `ProviderId` 作为接入层身份：

```rust
pub enum ProviderId {
    ClaudeCode,
    Cursor,
    Codex,
}
```

后续如果出现同一个工具的多种接入方式，可以扩展为：

```rust
pub struct ProviderKey {
    pub id: ProviderId,
    pub transport: ProviderTransport,
}
```

例如 Claude command hook、Claude HTTP hook、Codex rollout watcher、remote sidecar。

### ProviderCapabilities

每个 provider 必须声明能力，UI 只按能力显示按钮：

```rust
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
```

建议初始能力：

| Provider | 第一期能力 |
| --- | --- |
| Claude Code | `blocking_decision`, `approve`, `deny`, `ask`, `updated_input` |
| Cursor | `deny` 可实验；`approve/ask` 默认关闭，版本验证后再开 |
| Codex | `live_status`, `session_restore`；暂不直接 approve/deny |

### AgentEvent

把 watcher/hook 的输出统一成标准事件：

```rust
pub struct AgentEvent {
    pub event_id: String,
    pub provider: ProviderId,
    pub session_id: String,
    pub cwd: Option<PathBuf>,
    pub status: Option<Status>,
    pub event_type: AgentEventType,
    pub task_hint: Option<String>,
    pub tool_name: Option<String>,
    pub summary: Option<String>,
    pub pending_action: Option<PendingAction>,
    pub privacy: PrivacyLevel,
}
```

`StateAggregator` 新增 `apply_agent_event(event)`，第一期内部仍然调用现有 `add_session/update_session_status/set_task_name/set_last_tool_call`，保持行为不变。

### 迁移顺序

1. 新增 `agent_event.rs`，让 `http_server.rs` 和 `codex_watcher.rs` 先生成 `AgentEvent`，再由 aggregator 应用。
2. 把 `HookEvent::event_type_to_status` 拆到 `providers/claude.rs` 和 `providers/cursor.rs`。
3. 把 `parse_codex_line` 的状态映射包装进 `providers/codex.rs`。
4. 把 `hook_installer.rs` 拆成 provider installer：Claude / Cursor / WSL forwarding。
5. 加 provider registry，只负责启动 watcher、安装 hook、暴露 capabilities。
6. 再增加新 provider。

这样每一步都能跑现有测试，不需要一次性换血。

## 可操作黄灯

### PendingAction 数据模型

黄灯要拆成两层：`Status::Waiting` 决定灯亮黄，`PendingAction` 决定能不能点按钮。

```rust
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
```

```rust
pub enum PendingActionKind {
    ToolApproval,
    PermissionRequest,
    ShellExecution,
    McpExecution,
    FileRead,
    UserQuestion,
    StaleSession,
}

pub enum UserDecisionKind {
    Approve,
    Deny,
    AskInProvider,
    Defer,
    OpenProvider,
    Dismiss,
}
```

`SessionRef` 不需要塞完整 action，只暴露摘要：

```rust
pub struct PendingActionSummary {
    pub action_id: String,
    pub kind: PendingActionKind,
    pub title: String,
    pub decisions: Vec<UserDecisionKind>,
    pub expires_at_ms: i64,
}
```

完整 action 放在 `PendingActionStore`，避免把敏感原始 payload 广播给前端。

### Hook 阻塞决策流

当前 `/events` 保持兼容，新增 v2 阻塞接口：

```text
provider hook stdin
  -> src-hook 解析 provider / session / tool 元数据
  -> POST /provider-events?wait=decision
  -> Deva Light 创建 PendingAction 并通知 UI
  -> 用户在 drawer 点击 approve/deny
  -> Tauri command decide_pending_action(action_id, decision, reason)
  -> PendingActionStore 唤醒等待中的 HTTP 请求
  -> HTTP 返回 provider-specific stdout JSON
  -> src-hook 把 JSON 打印到 stdout
  -> 原 provider 继续/拒绝工具调用
```

这个流的重点是：`src-hook` 必须等待结果，并把结果写回 stdout。只 POST 一个普通事件不够。

### HTTP / IPC 接口

建议新增：

```text
POST /provider-events
```

请求体包含标准事件和可选 pending action。`wait=decision` 表示这是阻塞 hook，HTTP handler 最多等待一个短超时。

```text
GET /pending-actions
```

后续给手机 Web companion 用；第一期可以不做。

Tauri command：

```rust
decide_pending_action(action_id: String, decision: String, reason: Option<String>)
get_pending_action(action_id: String)
```

`confirm_session` 保持原语义，只用于 dismiss/ack，不参与 approve/deny。

### UI 行为

Drawer 中的 Waiting row：

- 如果 `pending_action.decisions` 包含 `Approve/Deny`，显示两个明确按钮。
- 如果只有 `OpenProvider`，显示“打开/定位”类按钮，不显示 approve。
- 点击行本身只展开详情，不直接批准。
- 黄灯主灯点击不执行 approve，只打开 drawer 或保持原来的确认行为。

原因：主灯太小，误触成本高；批准执行命令应该是明确按钮。

### 安全默认值

- 超时默认不 approve。Claude 可以返回 `ask` 或 `defer`；Cursor/Codex 默认退回 provider 自己的交互。
- `deny` 可本地和远程执行；`approve` 默认只允许本机 Tauri UI。远程 approve 以后必须单独开关。
- action id 需要随机 nonce，并绑定 session id、provider、event digest，避免旧按钮复用。
- Pending action 默认 90-120 秒过期，过期后 UI 按钮禁用。
- 不把完整 `tool_input` 长期写入日志。日志只写 event id、provider、tool、状态和脱敏摘要。
- 文件 diff 只在本机读取；SSH/WSL/LAN 先只显示路径和 tool 摘要。

## Provider 分级落地

### Claude Code：第一期可靠目标

Claude Code hooks 支持 command hook 从 stdin 读取事件，并通过 stdout JSON 返回决策。官方文档里 `PreToolUse` 支持 `permissionDecision` 的 `allow/deny/ask/defer`，`PermissionRequest` 支持 `decision.behavior` 的 `allow/deny`。

适合第一期：

- `PreToolUse`：显示工具名、命令/文件路径摘要，允许 `Approve/Deny/Ask`。
- `PermissionRequest`：显示权限请求，允许 `Approve/Deny`。
- `AskUserQuestion`/计划退出类场景：后续可用 `updatedInput` 处理，但不要放进第一期。

第一期只做：

```text
Claude PreToolUse / PermissionRequest
  -> PendingAction
  -> Drawer Approve / Deny
  -> provider-specific stdout JSON
```

### Cursor：先保守

Cursor hooks 可以观测和阻断部分 agent 行为，但公开社区记录显示 Cursor CLI/Agent hooks 在事件覆盖和 `allow/ask` 执行上有版本差异，且有记录指出 `ask/allow` 曾不稳定而 `deny` 更可靠。

建议第一期：

- 继续用 Cursor hook 做黄灯、工具摘要、等待原因。
- `beforeShellExecution/beforeMCPExecution/beforeReadFile` 可显示 `Deny`，但默认不显示 `Approve`。
- 增加一个隐藏配置或诊断探针，确认当前 Cursor 版本真的支持 `allow` 后再显示 approve。

### Codex：先不做直接 approve/deny

当前 Codex 接入来自 rollout JSONL 监听，不是阻塞 hook。看到 `request_user_input` 或 `sandbox_permissions=require_escalated` 时可以标黄，但没有一个同步通道把 Deva Light 的决策写回正在等待的 Codex CLI。

建议第一期：

- Waiting row 显示 `OpenProvider` / `Copy session id` / `Open project`。
- 如果以后 Codex 暴露 hook/IPC/approval socket，再通过 provider capability 打开 approve/deny。

## 第一批 PR 拆分

### PR 1：事件层，不改变行为

- 新增 `agent_event.rs`。
- `HookEvent` 和 `CodexLineEvent` 转换成 `AgentEvent`。
- `StateAggregator::apply_agent_event` 复用现有方法。
- 测试覆盖当前事件到 status 的映射。

### PR 2：Provider capabilities

- 新增 `providers/`。
- Claude/Cursor/Codex 返回 capabilities。
- UI 暂不显示新按钮，只在 diagnostics 中展示 provider capability。

### PR 3：PendingAction store

- 新增 `pending_action.rs`。
- `StateAggregator` 或单独 `PendingActionStore` 管理 action 生命周期。
- `SessionRef` 输出 `pending_action` 摘要。
- Drawer 显示等待原因，但按钮先禁用或只显示 `OpenProvider`。

### PR 4：Claude blocking decision

- `src-hook` 新增 v2 模式：阻塞等待 `/provider-events?wait=decision`。
- HTTP server 增加等待/唤醒逻辑。
- Tauri command `decide_pending_action`。
- Drawer 对 Claude 显示 `Approve/Deny`。
- 测试 fake provider：HTTP 请求阻塞，IPC 决策后返回 provider JSON。

### PR 5：Cursor deny-only 实验

- Cursor provider 标记 `deny=true`，`approve=false`。
- 只在 `before*` 类事件显示 `Deny`。
- 加版本/行为诊断，不通过则降级为只展示。

## 不建议做的事

- 不要把 `confirm_session` 改成 approve。它现在是状态确认，语义不同。
- 不要给所有黄灯都显示 approve。Codex 文件监听和 Cursor 不稳定场景会造成假按钮。
- 不要默认远程 approve。LAN token 只能证明来源，不代表用户确认了执行风险。
- 不要先做插件市场。内部 provider 边界稳定后再考虑外部化。

## 参考资料

- Claude Code Hooks reference: https://docs.anthropic.com/en/docs/claude-code/hooks
- Cursor hooks product note: https://cursor.com/blog/hooks-partners
- Cursor community note on CLI hook coverage: https://forum.cursor.com/t/cursor-cli-doesnt-send-all-events-defined-in-hooks/148316
- Cursor community note on `ask/allow` reliability: https://forum.cursor.com/t/beforeshellexecution-returns-permission-ask-but-sandboxed-agent-shell-still-runs-the-command-sandbox-true/155438/4
