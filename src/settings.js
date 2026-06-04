const invoke = window.__TAURI__?.core?.invoke;
const currentWindow =
  window.__TAURI__?.window?.getCurrentWindow?.() ?? window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.();

const bindSelect = document.getElementById("http-bind");
const portInput = document.getElementById("http-port");
const configPath = document.getElementById("config-path");
const runtimePort = document.getElementById("runtime-port");
const statusEl = document.getElementById("status");
const codexManualPathsInput = document.getElementById("codex-manual-paths");
const codexDetectedPaths = document.getElementById("codex-detected-paths");
const codexMissingPaths = document.getElementById("codex-missing-paths");
const recentLog = document.getElementById("recent-log");
const saveButton = document.getElementById("save");
const closeButton = document.getElementById("close");
const installIntegrationButton = document.getElementById("install-integration");
const removeIntegrationButton = document.getElementById("remove-integration");
const prepareUninstallButton = document.getElementById("prepare-uninstall");
const refreshDiagnosticsButton = document.getElementById("refresh-diagnostics");
const copyDiagnosticsButton = document.getElementById("copy-diagnostics");
const openAppLogButton = document.getElementById("open-app-log");

// Window settings
const alwaysOnTopCheckbox = document.getElementById("always-on-top");

// Notification settings
const notificationsEnabledCheckbox = document.getElementById("notifications-enabled");
const notifyWaitingCheckbox = document.getElementById("notify-waiting");
const notifyDoneCheckbox = document.getElementById("notify-done");
let lastDiagnostics = null;

saveButton.addEventListener("click", saveSettings);
closeButton.addEventListener("click", () => currentWindow?.close());
installIntegrationButton.addEventListener("click", installIntegration);
removeIntegrationButton.addEventListener("click", removeIntegration);
prepareUninstallButton.addEventListener("click", prepareUninstall);
refreshDiagnosticsButton.addEventListener("click", refreshDiagnostics);
copyDiagnosticsButton.addEventListener("click", copyDiagnostics);
openAppLogButton.addEventListener("click", openAppLog);

loadSettings();

async function loadSettings() {
  setBusy(true);

  try {
    const config = await invoke("get_app_config");
    ensureBindOption(config.httpBind);
    bindSelect.value = config.httpBind;
    portInput.value = config.httpPort ?? "";
    configPath.textContent = config.configPath;
    runtimePort.textContent = config.runtimePort ? String(config.runtimePort) : "未运行";

    // Window settings
    alwaysOnTopCheckbox.checked = config.alwaysOnTop ?? true;

    // Notification settings
    notificationsEnabledCheckbox.checked = config.notificationsEnabled ?? true;
    notifyWaitingCheckbox.checked = config.notifyOnWaiting ?? true;
    notifyDoneCheckbox.checked = config.notifyOnDone ?? false;
    codexManualPathsInput.value = (config.codexManualPaths ?? []).join("\n");

    await refreshDiagnostics();
    setStatus("");
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function saveSettings() {
  const httpPort = parsePort();
  if (httpPort === false) return;

  setBusy(true);

  try {
    await invoke("save_app_config_command", {
      update: {
        httpBind: bindSelect.value,
        httpPort,
        alwaysOnTop: alwaysOnTopCheckbox.checked,
        notificationsEnabled: notificationsEnabledCheckbox.checked,
        notifyOnWaiting: notifyWaitingCheckbox.checked,
        notifyOnDone: notifyDoneCheckbox.checked,
        codexManualPaths: parseCodexManualPaths(),
      },
    });

    // Apply always_on_top immediately
    await invoke("set_always_on_top", { enabled: alwaysOnTopCheckbox.checked });
    await refreshDiagnostics();

    setStatus("已保存。Codex 监控器将在约 1 秒后重载路径变更。");
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function installIntegration() {
  setBusy(true);

  try {
    await invoke("install_hooks_command");
    setStatus("Claude 集成已安装。请重启 Claude Code 以生效。");
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function removeIntegration() {
  const confirmed = confirm(
    "确定要从 Claude Code 设置中移除 Deva Light 钩子并删除辅助程序吗？",
  );
  if (!confirmed) return;

  setBusy(true);

  try {
    await invoke("remove_hooks_command");
    setStatus("Claude 集成已移除。请重启 Claude Code 以生效。");
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function prepareUninstall() {
  const choice = confirm(
    "选择卸载清理模式：\n\n" +
    "点击「确定」进行完整清理（删除所有配置文件）\n" +
    "点击「取消」保留配置（仅移除钩子和运行时文件）\n\n" +
    "推荐：完整清理以彻底删除。"
  );

  const keepConfig = !choice; // OK = full cleanup (keepConfig=false), Cancel = keep config

  const finalConfirm = confirm(
    keepConfig
      ? "这将移除 Claude 钩子和运行时文件，但保留 config.json 以便将来重新安装。\n\n确定继续？"
      : "这将完全删除所有 Deva Light 数据，包括钩子、配置和日志。\n\n确定继续？"
  );

  if (!finalConfirm) return;

  setBusy(true);

  try {
    await invoke("prepare_uninstall", { keepConfig });
    setStatus(
      keepConfig
        ? "部分清理完成。现在可以卸载应用。配置文件已保留以便重新安装。"
        : "完整清理完成。现在可以卸载应用。"
    );
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function refreshDiagnostics() {
  const diagnostics = await invoke("get_diagnostics");
  lastDiagnostics = diagnostics;

  renderPathList(
    codexDetectedPaths,
    diagnostics?.codex_sessions_paths,
    "当前无可用的 Codex 会话路径。",
  );
  renderPathList(
    codexMissingPaths,
    diagnostics?.codex_missing_paths,
    "无缺失路径。",
  );
  recentLog.textContent = diagnostics?.recent_log || "(空)";
}

async function copyDiagnostics() {
  if (!lastDiagnostics) {
    await refreshDiagnostics();
  }

  const text = diagnosticsText(lastDiagnostics);
  if (navigator.clipboard) {
    await navigator.clipboard.writeText(text);
    setStatus("诊断信息已复制。");
    return;
  }

  setStatus("此窗口无法访问剪贴板。", true);
}

async function openAppLog() {
  try {
    await invoke("open_app_log");
  } catch (error) {
    setStatus(String(error), true);
  }
}

function parsePort() {
  const value = portInput.value.trim();
  if (!value) return null;

  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    setStatus("端口必须为空或 1~65535 之间的整数。", true);
    portInput.focus();
    return false;
  }

  return port;
}

function parseCodexManualPaths() {
  const values = codexManualPathsInput.value
    .split(/\r?\n/)
    .map((value) => value.trim())
    .filter(Boolean);

  return [...new Set(values)];
}

function ensureBindOption(value) {
  if ([...bindSelect.options].some((option) => option.value === value)) {
    return;
  }

  const option = document.createElement("option");
  option.value = value;
  option.textContent = value;
  bindSelect.appendChild(option);
}

function renderPathList(container, values, emptyLabel) {
  container.replaceChildren();

  const items = Array.isArray(values) && values.length > 0 ? values : [emptyLabel];
  for (const value of items) {
    const item = document.createElement("li");
    item.textContent = value;
    if (!Array.isArray(values) || values.length === 0) {
      item.classList.add("empty");
    }
    container.appendChild(item);
  }
}

function diagnosticsText(diagnostics) {
  if (!diagnostics) {
    return "诊断信息不可用。";
  }

  const codexSessionPaths = Array.isArray(diagnostics.codex_sessions_paths)
    ? diagnostics.codex_sessions_paths
    : [diagnostics.codex_sessions_path].filter(Boolean);
  const codexMissingPaths = Array.isArray(diagnostics.codex_missing_paths)
    ? diagnostics.codex_missing_paths
    : [];
  const codexManualPaths = Array.isArray(diagnostics.codex_manual_paths)
    ? diagnostics.codex_manual_paths
    : [];

  return [
    "Deva Light 诊断信息",
    "",
    `配置目录: ${diagnostics.config_dir}`,
    `运行时: ${diagnostics.runtime_path}`,
    `锁文件: ${diagnostics.lock_path}`,
    `日志: ${diagnostics.log_path}`,
    `Claude 设置: ${diagnostics.claude_settings_path}`,
    `钩子程序: ${diagnostics.hook_binary_path}`,
    `Codex 会话: ${codexSessionPaths[0] || "(无)"}`,
    ...codexSessionPaths.slice(1).map((path) => `  - ${path}`),
    `Codex 自定义: ${codexManualPaths[0] || "(无)"}`,
    ...codexManualPaths.slice(1).map((path) => `  - ${path}`),
    `Codex 缺失: ${codexMissingPaths[0] || "(无)"}`,
    ...codexMissingPaths.slice(1).map((path) => `  - ${path}`),
    "",
    `钩子已安装: ${diagnostics.hooks_installed}`,
    `钩子程序存在: ${diagnostics.hook_binary_exists}`,
    `运行时存在: ${diagnostics.runtime_exists}`,
    `灯组数量: ${diagnostics.light_count}`,
    "",
    "最近日志:",
    diagnostics.recent_log || "(空)",
  ].join("\n");
}

function setBusy(isBusy) {
  saveButton.disabled = isBusy;
  closeButton.disabled = isBusy;
  installIntegrationButton.disabled = isBusy;
  removeIntegrationButton.disabled = isBusy;
  prepareUninstallButton.disabled = isBusy;
  refreshDiagnosticsButton.disabled = isBusy;
  copyDiagnosticsButton.disabled = isBusy;
  openAppLogButton.disabled = isBusy;
  bindSelect.disabled = isBusy;
  portInput.disabled = isBusy;
  codexManualPathsInput.disabled = isBusy;
  alwaysOnTopCheckbox.disabled = isBusy;
  notificationsEnabledCheckbox.disabled = isBusy;
  notifyWaitingCheckbox.disabled = isBusy;
  notifyDoneCheckbox.disabled = isBusy;
}

function setStatus(message, isError = false) {
  statusEl.textContent = message;
  statusEl.classList.toggle("error", isError);
}
