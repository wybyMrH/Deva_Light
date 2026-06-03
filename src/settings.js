const invoke = window.__TAURI__?.core?.invoke;
const currentWindow =
  window.__TAURI__?.window?.getCurrentWindow?.() ?? window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.();

const bindSelect = document.getElementById("http-bind");
const portInput = document.getElementById("http-port");
const configPath = document.getElementById("config-path");
const runtimePort = document.getElementById("runtime-port");
const statusEl = document.getElementById("status");
const saveButton = document.getElementById("save");
const closeButton = document.getElementById("close");
const installIntegrationButton = document.getElementById("install-integration");
const removeIntegrationButton = document.getElementById("remove-integration");
const prepareUninstallButton = document.getElementById("prepare-uninstall");

// Window settings
const alwaysOnTopCheckbox = document.getElementById("always-on-top");

// Notification settings
const notificationsEnabledCheckbox = document.getElementById("notifications-enabled");
const notifyWaitingCheckbox = document.getElementById("notify-waiting");
const notifyDoneCheckbox = document.getElementById("notify-done");

saveButton.addEventListener("click", saveSettings);
closeButton.addEventListener("click", () => currentWindow?.close());
installIntegrationButton.addEventListener("click", installIntegration);
removeIntegrationButton.addEventListener("click", removeIntegration);
prepareUninstallButton.addEventListener("click", prepareUninstall);

loadSettings();

async function loadSettings() {
  setBusy(true);

  try {
    const config = await invoke("get_app_config");
    ensureBindOption(config.httpBind);
    bindSelect.value = config.httpBind;
    portInput.value = config.httpPort ?? "";
    configPath.textContent = config.configPath;
    runtimePort.textContent = config.runtimePort ? String(config.runtimePort) : "Not running";

    // Window settings
    alwaysOnTopCheckbox.checked = config.alwaysOnTop ?? true;

    // Notification settings
    notificationsEnabledCheckbox.checked = config.notificationsEnabled ?? true;
    notifyWaitingCheckbox.checked = config.notifyOnWaiting ?? true;
    notifyDoneCheckbox.checked = config.notifyOnDone ?? false;

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
      },
    });

    // Apply always_on_top immediately
    await invoke("set_always_on_top", { enabled: alwaysOnTopCheckbox.checked });

    setStatus("Saved.");
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
    setStatus("Claude integration installed. Restart Claude Code to apply.");
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function removeIntegration() {
  const confirmed = confirm(
    "Remove Deva Light hooks from Claude Code settings and delete the hook helper?",
  );
  if (!confirmed) return;

  setBusy(true);

  try {
    await invoke("remove_hooks_command");
    setStatus("Claude integration removed. Restart Claude Code to apply.");
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

async function prepareUninstall() {
  const choice = confirm(
    "Choose uninstall cleanup mode:\n\n" +
    "Click OK for FULL CLEANUP (remove all config files)\n" +
    "Click Cancel for KEEP CONFIG (only remove hooks and runtime files)\n\n" +
    "Recommended: Full cleanup for complete removal."
  );

  const keepConfig = !choice; // OK = full cleanup (keepConfig=false), Cancel = keep config

  const finalConfirm = confirm(
    keepConfig
      ? "This will remove Claude hooks and runtime files, but keep your config.json for future reinstall.\n\nProceed?"
      : "This will completely remove all Deva Light data including hooks, config, and logs.\n\nProceed?"
  );

  if (!finalConfirm) return;

  setBusy(true);

  try {
    await invoke("prepare_uninstall", { keepConfig });
    setStatus(
      keepConfig
        ? "Partial cleanup complete. You can now uninstall the app. Config preserved for reinstall."
        : "Full cleanup complete. You can now uninstall the app."
    );
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    setBusy(false);
  }
}

function parsePort() {
  const value = portInput.value.trim();
  if (!value) return null;

  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    setStatus("Port must be blank or between 1 and 65535.", true);
    portInput.focus();
    return false;
  }

  return port;
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

function setBusy(isBusy) {
  saveButton.disabled = isBusy;
  closeButton.disabled = isBusy;
  installIntegrationButton.disabled = isBusy;
  removeIntegrationButton.disabled = isBusy;
  prepareUninstallButton.disabled = isBusy;
  bindSelect.disabled = isBusy;
  portInput.disabled = isBusy;
  alwaysOnTopCheckbox.disabled = isBusy;
  notificationsEnabledCheckbox.disabled = isBusy;
  notifyWaitingCheckbox.disabled = isBusy;
  notifyDoneCheckbox.disabled = isBusy;
}

function setStatus(message, isError = false) {
  statusEl.textContent = message;
  statusEl.classList.toggle("error", isError);
}