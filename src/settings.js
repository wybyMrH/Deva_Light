const invoke = window.__TAURI__?.core?.invoke;
const currentWindow =
  window.__TAURI__?.window?.getCurrentWindow?.() ??
  window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.();

const configPathEl = document.getElementById("config-path");
const runtimePortEl = document.getElementById("runtime-port");
const statusEl = document.getElementById("status");
const panelTitleEl = document.getElementById("panel-title");
const panelDescEl = document.getElementById("panel-desc");
const lanDetailsEl = document.getElementById("lan-details");
const notificationOptionsEl = document.getElementById("notification-options");

const portInput = document.getElementById("http-port");
const codexManualPathsInput = document.getElementById("codex-manual-paths");
const codexDetectedPaths = document.getElementById("codex-detected-paths");
const codexMissingPaths = document.getElementById("codex-missing-paths");
const recentLog = document.getElementById("recent-log");

const saveButton = document.getElementById("save");
const closeButton = document.getElementById("close");
const installIntegrationButton = document.getElementById("install-integration");
const installCursorIntegrationButton = document.getElementById("install-cursor-integration");
const removeIntegrationButton = document.getElementById("remove-integration");
const prepareUninstallButton = document.getElementById("prepare-uninstall");
const refreshDiagnosticsButton = document.getElementById("refresh-diagnostics");
const copyDiagnosticsButton = document.getElementById("copy-diagnostics");
const openAppLogButton = document.getElementById("open-app-log");

const alwaysOnTopCheckbox = document.getElementById("always-on-top");
const sshTargetsListEl = document.getElementById("ssh-targets-list");
const addSshTargetButton = document.getElementById("add-ssh-target");
const originAliasesInput = document.getElementById("origin-aliases");
const remoteCodexViaSshCheckbox = document.getElementById("remote-codex-via-ssh");
const sshCodexPathEl = document.getElementById("ssh-codex-path");
const diagnosticsPathsEl = document.getElementById("diagnostics-paths");
const httpTokenEl = document.getElementById("http-token");
const localAddressesEl = document.getElementById("local-addresses");
const remoteInstallCommand = document.getElementById("remote-install-command");
const refreshRemoteButton = document.getElementById("refresh-remote");
const copyInstallCommandButton = document.getElementById("copy-install-command");
const copySshCommandButton = document.getElementById("copy-ssh-command");
const regenerateTokenButton = document.getElementById("regenerate-token");

const notificationsEnabledCheckbox = document.getElementById("notifications-enabled");
const notifyWaitingCheckbox = document.getElementById("notify-waiting");
const notifyDoneCheckbox = document.getElementById("notify-done");

const appVersionEl = document.getElementById("app-version");
const checkUpdateButton = document.getElementById("check-update");
const updateCardEl = document.getElementById("update-card");
const updateVersionEl = document.getElementById("update-version");
const updateNotesEl = document.getElementById("update-notes");
const installUpdateButton = document.getElementById("install-update");
const updateStatusEl = document.getElementById("update-status");
const updateProgressWrapEl = document.getElementById("update-progress-wrap");
const updateProgressFillEl = document.getElementById("update-progress-fill");
const updateProgressTextEl = document.getElementById("update-progress-text");

const tauriEvent = window.__TAURI__?.event;

let lastDiagnostics = null;
let activePanel = "general";
let pendingUpdate = null;

document.querySelectorAll(".nav-item").forEach((button) => {
  button.addEventListener("click", () => switchPanel(button.dataset.panel));
});

document.querySelectorAll('input[name="http-bind"]').forEach((input) => {
  input.addEventListener("change", syncLanDetailsVisibility);
});

document.querySelectorAll('input[name="display-mode"]').forEach((input) => {
  input.addEventListener("change", applyDisplayMode);
});

async function applyDisplayMode() {
  try {
    await invoke("set_display_mode", { mode: getDisplayMode() });
  } catch (error) {
    console.debug("applyDisplayMode", error);
  }
}

notificationsEnabledCheckbox.addEventListener("change", syncNotificationOptions);

saveButton.addEventListener("click", saveSettings);
closeButton.addEventListener("click", closeSettings);
configPathEl.addEventListener("click", openConfigDir);
addSshTargetButton?.addEventListener("click", () => addSshTargetRow());
installIntegrationButton.addEventListener("click", installIntegration);
installCursorIntegrationButton?.addEventListener("click", installCursorIntegration);
removeIntegrationButton.addEventListener("click", removeIntegration);
prepareUninstallButton.addEventListener("click", prepareUninstall);
refreshDiagnosticsButton.addEventListener("click", refreshDiagnostics);
copyDiagnosticsButton.addEventListener("click", copyDiagnostics);
openAppLogButton.addEventListener("click", openAppLog);
refreshRemoteButton.addEventListener("click", refreshRemoteSetup);
copyInstallCommandButton.addEventListener("click", copyInstallCommand);
copySshCommandButton.addEventListener("click", copySshCommand);
regenerateTokenButton.addEventListener("click", regenerateToken);
checkUpdateButton.addEventListener("click", () => checkForUpdates(true));
installUpdateButton.addEventListener("click", installUpdate);

tauriEvent?.listen("open-settings-panel", (event) => {
  switchPanel(event.payload || "general");
});

tauriEvent?.listen("update-available", (event) => {
  showUpdateAvailable(event.payload);
  switchPanel("about");
});

tauriEvent?.listen("update-download-progress", (event) => {
  renderUpdateProgress(event.payload);
});

loadSettings();

function switchPanel(panelId) {
  if (!panelId) return;
  activePanel = panelId;

  document.querySelectorAll(".nav-item").forEach((button) => {
    button.classList.toggle("active", button.dataset.panel === panelId);
  });

  document.querySelectorAll(".panel").forEach((panel) => {
    panel.classList.toggle("active", panel.id === `panel-${panelId}`);
  });

  const active = document.getElementById(`panel-${panelId}`);
  if (active) {
    panelTitleEl.textContent = active.dataset.title || "";
    panelDescEl.textContent = active.dataset.desc || "";
  }

  if (panelId === "about") {
    void loadAppVersion();
  }

  if (panelId === "advanced") {
    void refreshDiagnostics();
  }
}

function getHttpBind() {
  return document.querySelector('input[name="http-bind"]:checked')?.value || "127.0.0.1";
}

function setHttpBind(value) {
  const input = document.querySelector(`input[name="http-bind"][value="${value}"]`);
  if (input) {
    input.checked = true;
  } else {
    const custom = document.createElement("input");
    custom.type = "radio";
    custom.name = "http-bind";
    custom.value = value;
    custom.hidden = true;
    document.body.appendChild(custom);
    custom.checked = true;
  }
  syncLanDetailsVisibility();
}

function getDisplayMode() {
  return document.querySelector('input[name="display-mode"]:checked')?.value || "parallel";
}

function setDisplayMode(value) {
  const input = document.querySelector(`input[name="display-mode"][value="${value}"]`);
  if (input) input.checked = true;
}

function syncLanDetailsVisibility() {
  const isLan = getHttpBind() === "0.0.0.0";
  lanDetailsEl.hidden = !isLan;
}

function syncNotificationOptions() {
  notificationOptionsEl.classList.toggle(
    "disabled",
    !notificationsEnabledCheckbox.checked,
  );
}

async function loadSettings() {
  setBusy(true);
  setStatus("正在加载设置…");

  try {
    const config = await invoke("get_app_config");
    setHttpBind(config.httpBind || "127.0.0.1");
    portInput.value = config.httpPort ?? "";
    const configDir = parentDir(config.configPath);
    configPathEl.textContent = configDir || config.configPath;
    configPathEl.title = `打开配置目录：${configDir || config.configPath}`;
    runtimePortEl.textContent = config.runtimePort ? String(config.runtimePort) : "未运行";

    alwaysOnTopCheckbox.checked = config.alwaysOnTop ?? true;
    setDisplayMode(config.displayMode || "parallel");
    renderSshTargets(config.remoteSshTargets || []);
    originAliasesInput.value = formatOriginAliases(config.originAliases || []);
    remoteCodexViaSshCheckbox.checked = config.remoteCodexViaSsh ?? true;
    httpTokenEl.textContent = config.httpToken || "未启用";

    notificationsEnabledCheckbox.checked = config.notificationsEnabled ?? true;
    notifyWaitingCheckbox.checked = config.notifyOnWaiting ?? true;
    notifyDoneCheckbox.checked = config.notifyOnDone ?? false;
    syncNotificationOptions();

    codexManualPathsInput.value = (config.codexManualPaths ?? []).join("\n");

    // Version and update check should not wait on slow diagnostics / remote probes.
    await loadAppVersion();
    void checkForUpdates(false);

    await Promise.allSettled([refreshDiagnostics(), refreshRemoteSetup()]);
    setStatus("");
  } catch (error) {
    setStatus(String(error), true);
    void loadAppVersion();
  } finally {
    setBusy(false);
  }
}

async function saveSettings() {
  const httpPort = parsePort();
  if (httpPort === false) return;

  setBusy(true);

  try {
    const result = await invoke("save_app_config_command", {
      update: {
        httpBind: getHttpBind(),
        httpPort,
        alwaysOnTop: alwaysOnTopCheckbox.checked,
        notificationsEnabled: notificationsEnabledCheckbox.checked,
        notifyOnWaiting: notifyWaitingCheckbox.checked,
        notifyOnDone: notifyDoneCheckbox.checked,
        codexManualPaths: parseCodexManualPaths(),
        displayMode: getDisplayMode(),
        remoteSshTargets: collectSshTargets(),
        remoteCodexViaSsh: remoteCodexViaSshCheckbox.checked,
        originAliases: parseOriginAliases(),
      },
    });

    await invoke("set_always_on_top", { enabled: alwaysOnTopCheckbox.checked });
    await refreshDiagnostics();
    await refreshRemoteSetup();

    if (result?.runtimePort) {
      runtimePortEl.textContent = String(result.runtimePort);
    }

    setStatus(
      result?.httpReloaded
        ? "已保存，HTTP 服务已热重载。"
        : "已保存。Codex 监控器将在约 1 秒后重载路径变更。",
    );
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function loadAppVersion() {
  if (!appVersionEl) return;

  const formatVersion = (value) => (value ? `v${String(value).replace(/^v/i, "")}` : null);

  try {
    const appVersion = await window.__TAURI__?.app?.getVersion?.();
    const formatted = formatVersion(appVersion);
    if (formatted) {
      appVersionEl.textContent = formatted;
      return;
    }
  } catch (error) {
    console.debug("loadAppVersion:getVersion", error);
  }

  try {
    if (!invoke) {
      appVersionEl.textContent = "不可用";
      return;
    }

    const version = await invoke("get_app_version");
    const formatted = formatVersion(version);
    appVersionEl.textContent = formatted || "未知";
  } catch (error) {
    appVersionEl.textContent = "读取失败";
    console.debug("loadAppVersion", error);
  }
}

async function checkForUpdates(manual) {
  if (manual) {
    setBusy(true);
    reportUpdate("正在检查更新…", null);
  }

  try {
    const update = await invoke("check_for_update");
    if (update) {
      showUpdateAvailable(update);
      if (manual) {
        reportUpdate(`发现新版本 ${update.version}`, true);
      }
      return update;
    }

    pendingUpdate = null;
    updateCardEl.hidden = true;
    if (manual) {
      reportUpdate("当前已是最新版本", true);
    } else {
      reportUpdate("");
    }
    return null;
  } catch (error) {
    if (manual) {
      reportUpdate(String(error), false);
    }
    return null;
  } finally {
    if (manual) {
      setBusy(false);
    }
  }
}

function showUpdateAvailable(update) {
  if (!update?.version) return;

  pendingUpdate = update;
  updateCardEl.hidden = false;
  updateVersionEl.textContent = `v${update.version}`;
  updateNotesEl.textContent =
    update.notes?.trim() || "此版本已发布，建议更新以获得最新修复与功能。";
  updateProgressWrapEl.hidden = true;
  installUpdateButton.disabled = false;
  installUpdateButton.textContent = "立即更新并重启";

  if (
    appVersionEl &&
    (appVersionEl.textContent === "—" ||
      appVersionEl.textContent === "读取失败" ||
      appVersionEl.textContent === "未知") &&
    update.currentVersion
  ) {
    appVersionEl.textContent = `v${update.currentVersion}`;
  }
}

async function installUpdate() {
  if (!pendingUpdate) {
    reportUpdate("没有可安装的更新", false);
    return;
  }

  const confirmed = confirm(
    `将下载并安装 v${pendingUpdate.version}，安装完成后应用会自动重启。\n\n` +
      "Windows 可能会弹出 UAC 安装提示，请留意任务栏。\n\n确定继续？",
  );
  if (!confirmed) return;

  setBusy(true);
  installUpdateButton.disabled = true;
  installUpdateButton.textContent = "正在更新…";
  updateProgressWrapEl.hidden = false;
  renderUpdateProgress({ downloaded: 0, total: null, phase: "downloading" });
  reportUpdate("正在下载更新包…", null);

  try {
    await invoke("download_and_install_update");
    reportUpdate("安装完成，正在重启…", true);
  } catch (error) {
    const message = String(error);
    reportUpdate(message, false);
    installUpdateButton.disabled = false;
    installUpdateButton.textContent = "立即更新并重启";
    void refreshDiagnostics();
  } finally {
    setBusy(false);
  }
}

function renderUpdateProgress(progress) {
  if (!progress) return;

  updateProgressWrapEl.hidden = false;

  if (progress.phase === "installing") {
    updateProgressFillEl.style.width = "100%";
    updateProgressTextEl.textContent = "下载完成，正在安装…";
    reportUpdate("正在安装更新…（如有 UAC 提示请允许）", null);
    return;
  }

  const downloaded = Number(progress.downloaded) || 0;
  const total = progress.total ? Number(progress.total) : null;

  if (total && total > 0) {
    const percent = Math.min(100, Math.round((downloaded / total) * 100));
    updateProgressFillEl.style.width = `${percent}%`;
    updateProgressTextEl.textContent = `已下载 ${formatBytes(downloaded)} / ${formatBytes(total)} (${percent}%)`;
  } else {
    updateProgressFillEl.style.width = "35%";
    updateProgressTextEl.textContent = `已下载 ${formatBytes(downloaded)}`;
  }
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function reportUpdate(message, ok) {
  setUpdateStatus(message, ok);
  setStatus(message, ok === false);
}

function renderSshTargets(targets) {
  sshTargetsListEl.replaceChildren();
  const rows = targets.length > 0 ? targets : [{ target: "", identityFile: null }];
  rows.forEach((entry) => addSshTargetRow(entry));
}

function formatOriginAliases(entries) {
  return entries
    .map((entry) => `${entry.key}=${entry.alias}`)
    .join("\n");
}

function parseOriginAliases() {
  return originAliasesInput.value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const index = line.indexOf("=");
      if (index <= 0) return null;
      return {
        key: line.slice(0, index).trim(),
        alias: line.slice(index + 1).trim(),
      };
    })
    .filter(Boolean);
}

function addSshTargetRow(entry = { target: "", identityFile: null, label: null }) {
  const row = document.createElement("div");
  row.className = "ssh-target-row";

  const targetField = document.createElement("label");
  targetField.className = "field";
  targetField.innerHTML = "<span>SSH 目标</span>";
  const targetInput = document.createElement("input");
  targetInput.type = "text";
  targetInput.placeholder = "user@192.168.1.10";
  targetInput.value = entry.target || "";
  targetField.appendChild(targetInput);

  const identityField = document.createElement("label");
  identityField.className = "field";
  identityField.innerHTML = "<span>私钥路径（可选）</span>";
  const identityInput = document.createElement("input");
  identityInput.type = "text";
  identityInput.placeholder = "~/.ssh/id_ed25519";
  identityInput.value = entry.identityFile || "";
  identityField.appendChild(identityInput);

  const labelField = document.createElement("label");
  labelField.className = "field";
  labelField.innerHTML = "<span>显示别名（可选）</span>";
  const labelInput = document.createElement("input");
  labelInput.type = "text";
  labelInput.placeholder = "公司机";
  labelInput.value = entry.label || "";
  labelField.appendChild(labelInput);

  const actions = document.createElement("div");
  actions.className = "ssh-target-actions";

  const testButton = document.createElement("button");
  testButton.type = "button";
  testButton.textContent = "测试";
  testButton.addEventListener("click", () =>
    testSshConnection(targetInput.value.trim(), identityInput.value.trim(), testButton),
  );

  const removeButton = document.createElement("button");
  removeButton.type = "button";
  removeButton.className = "danger";
  removeButton.textContent = "删除";
  removeButton.addEventListener("click", () => {
    if (sshTargetsListEl.children.length <= 1) {
      targetInput.value = "";
      identityInput.value = "";
      labelInput.value = "";
      return;
    }
    row.remove();
  });

  const status = document.createElement("output");
  status.className = "status-badge ssh-row-status";
  status.setAttribute("aria-live", "polite");

  actions.append(testButton, removeButton, status);
  row.append(targetField, identityField, labelField, actions);
  sshTargetsListEl.appendChild(row);
}

function collectSshTargets() {
  return [...sshTargetsListEl.querySelectorAll(".ssh-target-row")]
    .map((row) => {
      const inputs = row.querySelectorAll("input");
      const target = inputs[0]?.value.trim();
      const identityFile = inputs[1]?.value.trim();
      const label = inputs[2]?.value.trim();
      if (!target) return null;
      return {
        target,
        identityFile: identityFile || null,
        label: label || null,
      };
    })
    .filter(Boolean);
}

async function testSshConnection(sshTarget, sshIdentityFile, button) {
  if (!sshTarget) {
    setStatus("请先填写 SSH 目标。", true);
    return;
  }

  const statusEl = button.parentElement.querySelector(".ssh-row-status");
  button.disabled = true;
  statusEl.textContent = "测试中…";
  statusEl.classList.remove("ok", "error");

  try {
    const result = await invoke("test_ssh_connection", {
      sshTarget,
      sshIdentityFile: sshIdentityFile || null,
    });

    statusEl.textContent = result?.message || "测试完成";
    statusEl.classList.toggle("ok", Boolean(result?.ok));
    statusEl.classList.toggle("error", result?.ok === false);
    if (result?.codexPath) {
      sshCodexPathEl.textContent = result.codexPath;
    }
  } catch (error) {
    statusEl.textContent = String(error);
    statusEl.classList.add("error");
  } finally {
    button.disabled = false;
  }
}

async function closeSettings() {
  try {
    await invoke("hide_settings");
  } catch {
    await currentWindow?.hide?.();
    await currentWindow?.close?.();
  }
}

async function openConfigDir() {
  try {
    await invoke("open_config_dir");
  } catch (error) {
    setStatus(String(error), true);
  }
}

function parentDir(filePath) {
  if (!filePath) return "";
  const normalized = String(filePath).replace(/[\\/]+$/, "");
  const index = Math.max(normalized.lastIndexOf("/"), normalized.lastIndexOf("\\"));
  if (index <= 0) return normalized;
  return normalized.slice(0, index);
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

async function installCursorIntegration() {
  setBusy(true);
  try {
    await invoke("install_cursor_hooks_command");
    setStatus("Cursor 集成已安装。请重启 Cursor 以生效。");
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function removeIntegration() {
  const confirmed = confirm(
    "确定要移除 Claude Code 与 Cursor 的 Deva Light 钩子并删除辅助程序吗？",
  );
  if (!confirmed) return;

  setBusy(true);
  try {
    await invoke("remove_hooks_command");
    setStatus("全部集成已移除。请重启 Claude Code / Cursor 以生效。");
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
      "推荐：完整清理以彻底删除。",
  );

  const keepConfig = !choice;
  const finalConfirm = confirm(
    keepConfig
      ? "这将移除 Claude 钩子和运行时文件，但保留 config.json 以便将来重新安装。\n\n确定继续？"
      : "这将完全删除所有 Deva Light 数据，包括钩子、配置和日志。\n\n确定继续？",
  );

  if (!finalConfirm) return;

  setBusy(true);
  try {
    await invoke("prepare_uninstall", { keepConfig });
    setStatus(
      keepConfig
        ? "部分清理完成。现在可以卸载应用。配置文件已保留以便重新安装。"
        : "完整清理完成。现在可以卸载应用。",
    );
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function refreshRemoteSetup() {
  try {
    const remote = await invoke("get_remote_setup_info");
    httpTokenEl.textContent = remote?.httpToken || "未启用";
    localAddressesEl.textContent =
      remote?.localAddresses?.join(", ") || remote?.primaryHost || "未检测到";
    remoteInstallCommand.value =
      remote?.curlInstallCommand || remote?.installCommand || "";
    const sshPaths = (remote?.sshTargets || [])
      .map((entry) => entry.sshCodexPath)
      .filter(Boolean);
    sshCodexPathEl.textContent =
      sshPaths.join("\n") || remote?.sshCodexPath || "未检测到（请先测试 SSH 连接）";
    copySshCommandButton.disabled = !remote?.sshInstallCommand;
    window.__lastRemoteSetup = remote;
  } catch (error) {
    localAddressesEl.textContent = String(error);
    remoteInstallCommand.value = "";
  }
}

async function copyInstallCommand() {
  const text = remoteInstallCommand.value.trim();
  if (!text) {
    setStatus("安装命令不可用。请先启用局域网转发并刷新远程信息。", true);
    return;
  }
  if (navigator.clipboard) {
    await navigator.clipboard.writeText(text);
    setStatus("Ubuntu 安装命令已复制。");
  }
}

async function copySshCommand() {
  const text = window.__lastRemoteSetup?.sshInstallCommand;
  if (!text) {
    setStatus("请填写 SSH 目标并刷新远程信息。", true);
    return;
  }
  if (navigator.clipboard) {
    await navigator.clipboard.writeText(text);
    setStatus("SSH 安装命令已复制。");
  }
}

async function regenerateToken() {
  const confirmed = confirm("重新生成 Token 后，远程 hook 需要重新安装。确定继续？");
  if (!confirmed) return;

  setBusy(true);
  try {
    const httpPort = parsePort();
    if (httpPort === false) return;

    const result = await invoke("save_app_config_command", {
      update: {
        httpBind: getHttpBind(),
        httpPort,
        displayMode: getDisplayMode(),
        remoteSshTargets: collectSshTargets(),
        remoteCodexViaSsh: remoteCodexViaSshCheckbox.checked,
        originAliases: parseOriginAliases(),
        regenerateHttpToken: true,
      },
    });
    await refreshRemoteSetup();
    if (result?.runtimePort) {
      runtimePortEl.textContent = String(result.runtimePort);
    }
    setStatus(
      result?.httpReloaded
        ? "Token 已重新生成，HTTP 服务已热重载。请重新安装远程 hook。"
        : "Token 已重新生成。请重新安装远程 hook。",
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
    diagnostics?.codexSessionsPaths,
    "当前无可用的 Codex 会话路径。",
  );
  renderPathList(
    codexMissingPaths,
    diagnostics?.codexMissingPaths,
    "无缺失路径。",
  );
  renderDiagnosticsPaths(diagnostics);
  recentLog.textContent = diagnostics?.recentLog || "(空)";
}

function renderDiagnosticsPaths(diagnostics) {
  diagnosticsPathsEl.replaceChildren();
  if (!diagnostics) return;

  const entries = [
    ["配置目录", diagnostics.configDir],
    ["运行时", diagnostics.runtimePath],
    ["锁文件", diagnostics.lockPath],
    ["日志文件", diagnostics.logPath],
    ["Claude 设置", diagnostics.claudeSettingsPath],
    ["钩子程序", diagnostics.hookBinaryPath],
  ];

  for (const [label, path] of entries) {
    if (!path) continue;

    const row = document.createElement("div");
    row.className = "diagnostics-path-row";

    const term = document.createElement("dt");
    term.textContent = label;

    const value = document.createElement("dd");
    const link = document.createElement("button");
    link.type = "button";
    link.className = "path-link";
    link.textContent = path;
    link.title = `打开：${path}`;
    link.addEventListener("click", () => openPathInExplorer(path));
    value.appendChild(link);

    row.append(term, value);
    diagnosticsPathsEl.appendChild(row);
  }
}

async function openPathInExplorer(path) {
  try {
    await invoke("open_path_in_explorer", { path });
  } catch (error) {
    setStatus(String(error), true);
  }
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
    await invoke("open_config_dir");
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

  const codexSessionPaths = Array.isArray(diagnostics.codexSessionsPaths)
    ? diagnostics.codexSessionsPaths
    : [diagnostics.codexSessionsPath].filter(Boolean);
  const codexMissingPathsList = Array.isArray(diagnostics.codexMissingPaths)
    ? diagnostics.codexMissingPaths
    : [];
  const codexManualPaths = Array.isArray(diagnostics.codexManualPaths)
    ? diagnostics.codexManualPaths
    : [];

  return [
    "Deva Light 诊断信息",
    "",
    `配置目录: ${diagnostics.configDir}`,
    `运行时: ${diagnostics.runtimePath}`,
    `锁文件: ${diagnostics.lockPath}`,
    `日志: ${diagnostics.logPath}`,
    `Claude 设置: ${diagnostics.claudeSettingsPath}`,
    `钩子程序: ${diagnostics.hookBinaryPath}`,
    `Codex 会话: ${codexSessionPaths[0] || "(无)"}`,
    ...codexSessionPaths.slice(1).map((path) => `  - ${path}`),
    `Codex 自定义: ${codexManualPaths[0] || "(无)"}`,
    ...codexManualPaths.slice(1).map((path) => `  - ${path}`),
    `Codex 缺失: ${codexMissingPathsList[0] || "(无)"}`,
    ...codexMissingPathsList.slice(1).map((path) => `  - ${path}`),
    "",
    `钩子已安装: ${diagnostics.hooksInstalled}`,
    `钩子程序存在: ${diagnostics.hookBinaryExists}`,
    `运行时存在: ${diagnostics.runtimeExists}`,
    `灯组数量: ${diagnostics.lightCount}`,
    "",
    "最近日志:",
    diagnostics.recentLog || "(空)",
  ].join("\n");
}

function setControlDisabled(element, disabled) {
  if (element) {
    element.disabled = disabled;
  }
}

function setBusy(isBusy) {
  setControlDisabled(saveButton, isBusy);
  setControlDisabled(installIntegrationButton, isBusy);
  setControlDisabled(installCursorIntegrationButton, isBusy);
  setControlDisabled(removeIntegrationButton, isBusy);
  setControlDisabled(prepareUninstallButton, isBusy);
  setControlDisabled(refreshDiagnosticsButton, isBusy);
  setControlDisabled(copyDiagnosticsButton, isBusy);
  setControlDisabled(openAppLogButton, isBusy);
  setControlDisabled(portInput, isBusy);
  setControlDisabled(codexManualPathsInput, isBusy);
  setControlDisabled(alwaysOnTopCheckbox, isBusy);
  setControlDisabled(notificationsEnabledCheckbox, isBusy);
  setControlDisabled(notifyWaitingCheckbox, isBusy);
  setControlDisabled(notifyDoneCheckbox, isBusy);
  setControlDisabled(originAliasesInput, isBusy);
  setControlDisabled(remoteCodexViaSshCheckbox, isBusy);
  setControlDisabled(addSshTargetButton, isBusy);
  setControlDisabled(refreshRemoteButton, isBusy);
  setControlDisabled(copyInstallCommandButton, isBusy);
  setControlDisabled(copySshCommandButton, isBusy);
  setControlDisabled(regenerateTokenButton, isBusy);
  setControlDisabled(checkUpdateButton, isBusy);

  document.querySelectorAll('input[name="display-mode"]').forEach((input) => {
    input.disabled = isBusy;
  });
  document.querySelectorAll('input[name="http-bind"]').forEach((input) => {
    input.disabled = isBusy;
  });
  sshTargetsListEl?.querySelectorAll("input, button").forEach((element) => {
    element.disabled = isBusy;
  });

  if (installUpdateButton) {
    installUpdateButton.disabled = isBusy || !pendingUpdate;
  }
}

function setStatus(message, isError = false) {
  if (!statusEl) return;
  statusEl.textContent = message || "";
  statusEl.classList.toggle("error", isError);
}

function setUpdateStatus(message, ok) {
  if (!updateStatusEl) return;
  updateStatusEl.textContent = message || "";
  updateStatusEl.classList.remove("ok", "error");
  if (ok === true) updateStatusEl.classList.add("ok");
  if (ok === false) updateStatusEl.classList.add("error");
}
