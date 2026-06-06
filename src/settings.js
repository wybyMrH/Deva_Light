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
const remoteSshTargetInput = document.getElementById("remote-ssh-target");
const remoteCodexViaSshCheckbox = document.getElementById("remote-codex-via-ssh");
const sshIdentityFileInput = document.getElementById("ssh-identity-file");
const sshCodexPathEl = document.getElementById("ssh-codex-path");
const httpTokenEl = document.getElementById("http-token");
const localAddressesEl = document.getElementById("local-addresses");
const remoteInstallCommand = document.getElementById("remote-install-command");
const refreshRemoteButton = document.getElementById("refresh-remote");
const copyInstallCommandButton = document.getElementById("copy-install-command");
const copySshCommandButton = document.getElementById("copy-ssh-command");
const regenerateTokenButton = document.getElementById("regenerate-token");
const testSshButton = document.getElementById("test-ssh");
const sshTestStatusEl = document.getElementById("ssh-test-status");

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

notificationsEnabledCheckbox.addEventListener("change", syncNotificationOptions);

saveButton.addEventListener("click", saveSettings);
closeButton.addEventListener("click", () => currentWindow?.close());
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
testSshButton.addEventListener("click", testSshConnection);
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

  try {
    const config = await invoke("get_app_config");
    setHttpBind(config.httpBind || "127.0.0.1");
    portInput.value = config.httpPort ?? "";
    configPathEl.textContent = config.configPath;
    configPathEl.title = config.configPath;
    runtimePortEl.textContent = config.runtimePort ? String(config.runtimePort) : "未运行";

    alwaysOnTopCheckbox.checked = config.alwaysOnTop ?? true;
    setDisplayMode(config.displayMode || "parallel");
    remoteSshTargetInput.value = config.remoteSshTarget || "";
    remoteCodexViaSshCheckbox.checked = config.remoteCodexViaSsh ?? true;
    sshIdentityFileInput.value = config.sshIdentityFile || "";
    httpTokenEl.textContent = config.httpToken || "未启用";

    notificationsEnabledCheckbox.checked = config.notificationsEnabled ?? true;
    notifyWaitingCheckbox.checked = config.notifyOnWaiting ?? true;
    notifyDoneCheckbox.checked = config.notifyOnDone ?? false;
    syncNotificationOptions();

    codexManualPathsInput.value = (config.codexManualPaths ?? []).join("\n");

    await refreshDiagnostics();
    await refreshRemoteSetup();
    await loadAppVersion();
    await checkForUpdates(false);
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
        remoteSshTarget: remoteSshTargetInput.value.trim(),
        remoteCodexViaSsh: remoteCodexViaSshCheckbox.checked,
        sshIdentityFile: sshIdentityFileInput.value.trim(),
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
  try {
    appVersionEl.textContent = await invoke("get_app_version");
  } catch {
    appVersionEl.textContent = "未知";
  }
}

async function checkForUpdates(manual) {
  if (manual) {
    setUpdateStatus("正在检查更新…", null);
  }

  try {
    const update = await invoke("check_for_update");
    if (update) {
      showUpdateAvailable(update);
      if (manual) {
        setUpdateStatus(`发现新版本 ${update.version}`, true);
      }
      return update;
    }

    pendingUpdate = null;
    updateCardEl.hidden = true;
    if (manual) {
      setUpdateStatus("当前已是最新版本", true);
    } else {
      setUpdateStatus("");
    }
    return null;
  } catch (error) {
    if (manual) {
      setUpdateStatus(String(error), false);
    }
    return null;
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
}

async function installUpdate() {
  if (!pendingUpdate) {
    setUpdateStatus("没有可安装的更新", false);
    return;
  }

  const confirmed = confirm(
    `将下载并安装 v${pendingUpdate.version}，安装完成后应用会自动重启。\n\n确定继续？`,
  );
  if (!confirmed) return;

  setBusy(true);
  installUpdateButton.disabled = true;
  installUpdateButton.textContent = "正在更新…";
  updateProgressWrapEl.hidden = false;
  renderUpdateProgress({ downloaded: 0, total: null, phase: "downloading" });
  setUpdateStatus("正在下载更新包…", null);

  try {
    await invoke("download_and_install_update");
  } catch (error) {
    setUpdateStatus(String(error), false);
    installUpdateButton.disabled = false;
    installUpdateButton.textContent = "立即更新并重启";
    setBusy(false);
  }
}

function renderUpdateProgress(progress) {
  if (!progress) return;

  updateProgressWrapEl.hidden = false;

  if (progress.phase === "installing") {
    updateProgressFillEl.style.width = "100%";
    updateProgressTextEl.textContent = "下载完成，正在安装…";
    setUpdateStatus("正在安装更新…", null);
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

function setUpdateStatus(message, ok) {
  updateStatusEl.textContent = message;
  updateStatusEl.classList.remove("ok", "error");
  if (ok === true) updateStatusEl.classList.add("ok");
  if (ok === false) updateStatusEl.classList.add("error");
}

async function testSshConnection() {
  setBusy(true);
  setSshTestStatus("测试中…", null);

  try {
    const result = await invoke("test_ssh_connection", {
      sshTarget: remoteSshTargetInput.value.trim() || null,
      sshIdentityFile: sshIdentityFileInput.value.trim() || null,
    });

    setSshTestStatus(result?.message || "测试完成", result?.ok);
    if (result?.codexPath) {
      sshCodexPathEl.textContent = result.codexPath;
    }
  } catch (error) {
    setSshTestStatus(String(error), false);
  } finally {
    setBusy(false);
  }
}

function setSshTestStatus(message, ok) {
  sshTestStatusEl.textContent = message;
  sshTestStatusEl.classList.remove("ok", "error");
  if (ok === true) sshTestStatusEl.classList.add("ok");
  if (ok === false) sshTestStatusEl.classList.add("error");
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
    sshCodexPathEl.textContent =
      remote?.sshCodexPath || "未检测到（请先测试 SSH 连接）";
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
        remoteSshTarget: remoteSshTargetInput.value.trim(),
        remoteCodexViaSsh: remoteCodexViaSshCheckbox.checked,
        sshIdentityFile: sshIdentityFileInput.value.trim(),
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
  recentLog.textContent = diagnostics?.recentLog || "(空)";
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

function setBusy(isBusy) {
  saveButton.disabled = isBusy;
  closeButton.disabled = isBusy;
  installIntegrationButton.disabled = isBusy;
  if (installCursorIntegrationButton) {
    installCursorIntegrationButton.disabled = isBusy;
  }
  removeIntegrationButton.disabled = isBusy;
  prepareUninstallButton.disabled = isBusy;
  refreshDiagnosticsButton.disabled = isBusy;
  copyDiagnosticsButton.disabled = isBusy;
  openAppLogButton.disabled = isBusy;
  portInput.disabled = isBusy;
  codexManualPathsInput.disabled = isBusy;
  alwaysOnTopCheckbox.disabled = isBusy;
  notificationsEnabledCheckbox.disabled = isBusy;
  notifyWaitingCheckbox.disabled = isBusy;
  notifyDoneCheckbox.disabled = isBusy;
  document.querySelectorAll('input[name="display-mode"]').forEach((input) => {
    input.disabled = isBusy;
  });
  document.querySelectorAll('input[name="http-bind"]').forEach((input) => {
    input.disabled = isBusy;
  });
  remoteSshTargetInput.disabled = isBusy;
  remoteCodexViaSshCheckbox.disabled = isBusy;
  sshIdentityFileInput.disabled = isBusy;
  refreshRemoteButton.disabled = isBusy;
  copyInstallCommandButton.disabled = isBusy;
  copySshCommandButton.disabled = isBusy;
  regenerateTokenButton.disabled = isBusy;
  testSshButton.disabled = isBusy;
  checkUpdateButton.disabled = isBusy;
  installUpdateButton.disabled = isBusy || !pendingUpdate;
}

function setStatus(message, isError = false) {
  statusEl.textContent = message;
  statusEl.classList.toggle("error", isError);
}
