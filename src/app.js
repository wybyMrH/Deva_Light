import {
  createDrawer,
  updateProjectDrawer,
  showProjectDrawer,
  hideDrawer,
  getBadgeText,
  shouldShowBadge,
  isDrawerOpen,
  getDrawerMode,
} from "./drawer.js";

const tauriEvent = window.__TAURI__?.event;
const tauriCore = window.__TAURI__?.core;
const currentWindow =
  window.__TAURI__?.window?.getCurrentWindow?.() ??
  window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.();

let lights = [];
let displayMode = "parallel";
let compactFocusProjectId = null;
const lightElements = new Map();
let lastWindowSize = { width: 0, height: 0 };
let resizeFrame = 0;
const WINDOW_GUTTER_X = 36;
const WINDOW_GUTTER_Y = 28;
const MENU_EDGE_GUTTER = 12;
const DRAWER_EXTRA_GUTTER = 24;
const DRAG_THRESHOLD_PX = 5;

const container = document.getElementById("lights-container");
const menu = document.getElementById("menu");
const drawer = document.getElementById("drawer");
const appHandle = createAppHandle();
container.appendChild(appHandle);

// Initialize drawer content
drawer.appendChild(createDrawer());

tauriEvent?.listen("state-changed", (event) => {
  lights = Array.isArray(event.payload) ? event.payload : [];
  render();
});

tauriEvent?.listen("config-changed", (event) => {
  const nextMode = event.payload?.displayMode || "parallel";
  if (nextMode !== displayMode) {
    displayMode = nextMode;
    compactFocusProjectId = null;
    hideDrawer();
  }
  render();
});

window.addEventListener("drawer-project-selected", (event) => {
  const projectId = event.detail?.projectId;
  const light = lights.find((entry) => entry.project_id === projectId);
  if (!light) return;

  compactFocusProjectId = projectId;
  hideDrawer();
  render();
});

window.addEventListener("drawer-visibility-changed", () => {
  scheduleWindowResize();
});

document.addEventListener("click", (event) => {
  if (menu.contains(event.target)) {
    return;
  }

  if (drawer.contains(event.target)) {
    hideMenu();
    return;
  }

  hideMenu();

  if (isDrawerOpen() && !event.target.closest(".traffic-light")) {
    hideDrawer();
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    hideMenu();
    hideDrawer();
  }
});

let isDragging = false;
let dragStart = null;
let suppressClick = false;
let pendingDrag = null;

document.addEventListener("pointerdown", async (event) => {
  if (!shouldStartDrag(event)) return;

  // Standby: native drag on pointerdown. Project lights: defer so clicks still work.
  if (event.target.closest(".traffic-light--app")) {
    await beginWindowDrag(event);
    return;
  }

  pendingDrag = {
    pointerId: event.pointerId,
    startX: event.screenX,
    startY: event.screenY,
  };
  try {
    event.currentTarget.setPointerCapture?.(event.pointerId);
  } catch {}
});

document.addEventListener("pointermove", async (event) => {
  if (pendingDrag && event.pointerId === pendingDrag.pointerId) {
    const dx = event.screenX - pendingDrag.startX;
    const dy = event.screenY - pendingDrag.startY;
    if (Math.hypot(dx, dy) >= DRAG_THRESHOLD_PX) {
      pendingDrag = null;
      suppressClick = true;
      await beginWindowDrag(event);
    }
    return;
  }

  if (!isDragging || !dragStart || !currentWindow) return;

  const dx = event.screenX - dragStart.mouseX;
  const dy = event.screenY - dragStart.mouseY;

  try {
    const PhysicalPosition = window.__TAURI__?.dpi?.PhysicalPosition;
    if (PhysicalPosition) {
      await currentWindow.setPosition(
        new PhysicalPosition(dragStart.winX + dx, dragStart.winY + dy),
      );
    }
  } catch {}
});

document.addEventListener("pointerup", async (event) => {
  if (pendingDrag && event.pointerId === pendingDrag.pointerId) {
    pendingDrag = null;
  }

  if (isDragging) {
    try {
      const pos = await currentWindow?.outerPosition();
      if (pos) {
        await safeInvoke("persist_window_position", { x: pos.x, y: pos.y });
      }
    } catch {}
  }

  isDragging = false;
  dragStart = null;
});

async function beginWindowDrag(event) {
  try {
    await currentWindow?.startDragging?.();
    return;
  } catch {}

  isDragging = true;
  const pos = await currentWindow?.outerPosition();
  dragStart = {
    mouseX: event.screenX,
    mouseY: event.screenY,
    winX: pos?.x ?? 0,
    winY: pos?.y ?? 0,
  };
  try {
    event.target.setPointerCapture(event.pointerId);
  } catch {}
}

function statusPriority(status) {
  return ({ Waiting: 3, Working: 2, Done: 1, Idle: 0 }[status] ?? 0);
}

function lightsForDisplay() {
  if (displayMode === "compact" && lights.length > 1) {
    const sorted = [...lights].sort(
      (left, right) => statusPriority(right.status) - statusPriority(left.status),
    );
    if (
      !compactFocusProjectId ||
      !lights.some((light) => light.project_id === compactFocusProjectId)
    ) {
      compactFocusProjectId = sorted[0].project_id;
    }
    const focused =
      lights.find((light) => light.project_id === compactFocusProjectId) || sorted[0];
    return [focused];
  }

  return lights;
}

function cycleCompactProject() {
  if (displayMode !== "compact" || lights.length <= 1) {
    return;
  }

  const sorted = [...lights].sort(
    (left, right) => statusPriority(right.status) - statusPriority(left.status),
  );
  const currentIndex = sorted.findIndex(
    (light) => light.project_id === compactFocusProjectId,
  );
  const nextIndex = currentIndex < 0 ? 0 : (currentIndex + 1) % sorted.length;
  compactFocusProjectId = sorted[nextIndex].project_id;
  hideDrawer();
  render();
}

function render() {
  const displayLights = lightsForDisplay();
  const visibleProjectIds = new Set(displayLights.map((light) => light.project_id));
  appHandle.hidden = lights.length > 0;

  for (const light of displayLights) {
    let element = lightElements.get(light.project_id);
    if (!element) {
      element = createProjectLight(light);
      lightElements.set(light.project_id, element);
      container.appendChild(element);
    }

    updateProjectLight(element, light);
  }

  for (const [projectId, element] of lightElements) {
    if (!visibleProjectIds.has(projectId)) {
      element.remove();
      lightElements.delete(projectId);
    }
  }

  for (const [projectId, element] of lightElements) {
    element.classList.toggle(
      "is-drawer-active",
      isDrawerOpen() &&
        getDrawerMode() === "projects" &&
        displayMode === "compact" &&
        projectId === compactFocusProjectId,
    );
  }

  if (
    displayMode === "parallel" &&
    isDrawerOpen() &&
    getDrawerMode() !== "projects"
  ) {
    hideDrawer();
  }

  if (
    isDrawerOpen() &&
    getDrawerMode() === "projects" &&
    displayMode === "compact"
  ) {
    updateProjectDrawer(drawer, lights);
  }

  scheduleWindowResize();
}

function createAppHandle() {
  const root = createLightElement({
    label: "Deva Light",
    status: "Standby",
    title: "Deva Light\n拖动移动窗口 · 右键打开菜单",
    standby: true,
  });
  root.classList.add("traffic-light--app");

  root.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    showMenu(event.clientX, event.clientY, [
      ["设置", () => safeInvoke("open_settings", { panel: null })],
      ["检查更新", () => safeInvoke("open_settings", { panel: "about" })],
      ["诊断", () => showDiagnostics()],
      ["退出", () => safeInvoke("quit_app")],
    ]);
  });

  return root;
}

function createProjectLight(lightState) {
  const root = createLightElement({
    label: lightState.project_label,
    status: lightState.status,
    title: tooltipFor(lightState),
  });
  root.dataset.projectId = lightState.project_id;
  const origin = lightState.monitor_origin || lightState.monitorOrigin || "local";
  root.classList.add(`origin-${String(origin).toLowerCase()}`);

  // Add session badge
  const badge = document.createElement("div");
  badge.className = "session-badge hidden";
  root.appendChild(badge);

  root.addEventListener("click", (event) => {
    if (suppressClick) {
      suppressClick = false;
      return;
    }

    const projectId = root.dataset.projectId;
    const status = root.dataset.status;
    const sessions = root.lightState?.sessions || [];

    if (displayMode === "compact" && lights.length > 1) {
      event.stopPropagation();
      hideMenu();

      if (event.target.closest(".session-badge")) {
        if (isDrawerOpen() && getDrawerMode() === "projects") {
          hideDrawer();
        } else {
          showProjectDrawer(drawer, lights);
        }
        scheduleWindowResize();
        return;
      }

      if (
        sessions.length === 1 &&
        (status === "Waiting" || status === "Done") &&
        event.target.closest(".lamp.on")
      ) {
        safeInvoke("confirm_light", { projectId });
        return;
      }

      cycleCompactProject();
      return;
    }

    if (displayMode !== "compact" && sessions.length > 1) {
      event.stopPropagation();
      hideMenu();

      const chip = event.target.closest(".tool-chip");
      if (chip) {
        const chipIndex = [...root.querySelectorAll(".tool-chip")].indexOf(chip);
        const session = sessions[chipIndex];
        if (
          session &&
          (session.status === "Waiting" || session.status === "Done")
        ) {
          safeInvoke("confirm_session", { sessionId: session.session_id });
        }
        return;
      }

      if (status === "Waiting" || status === "Done") {
        safeInvoke("confirm_light", { projectId });
      }
      return;
    }

    if (status === "Waiting" || status === "Done") {
      safeInvoke("confirm_light", { projectId });
    }
  });

  root.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (isDrawerOpen()) {
      hideDrawer();
    }
    const projectId = root.dataset.projectId;
    showMenu(event.clientX, event.clientY, [
      ["打开", () => safeInvoke("open_project", { projectId })],
      ["复制路径", () => copyProjectPath(projectId)],
      ["设置", () => safeInvoke("open_settings")],
      ["移除", () => safeInvoke("remove_light", { projectId })],
    ]);
  });

  updateProjectLight(root, lightState);
  return root;
}

function createLightElement({ label, status, title, standby = false }) {
  const root = document.createElement("section");
  root.className = `traffic-light${standby ? " standby" : ""}`;
  root.title = title;
  root.dataset.status = status;

  const housing = document.createElement("div");
  housing.className = "light-housing";

  const originChip = document.createElement("span");
  originChip.className = "origin-chip hidden";
  housing.appendChild(originChip);

  housing.appendChild(createLamp("red", status === "Done" || status === "Idle"));
  housing.appendChild(createLamp("yellow", status === "Waiting"));
  housing.appendChild(createLamp("green", status === "Working"));

  const sessionChips = document.createElement("div");
  sessionChips.className = "session-chips";
  housing.appendChild(sessionChips);

  const labelEl = document.createElement("div");
  labelEl.className = "light-label";
  labelEl.textContent = label || "unknown";

  root.append(labelEl, housing);
  return root;
}

function updateProjectLight(root, lightState) {
  root.lightState = lightState;
  root.dataset.projectId = lightState.project_id;
  root.dataset.status = lightState.status;
  root.title = tooltipFor(lightState);

  const origin = lightState.monitor_origin || lightState.monitorOrigin || "local";
  root.classList.remove("origin-local", "origin-wsl", "origin-ssh", "origin-remote");
  root.classList.add(`origin-${String(origin).toLowerCase()}`);
  root.classList.toggle(
    "is-actionable",
    lightState.status === "Waiting" || lightState.status === "Done",
  );

  const label = root.querySelector(".light-label");
  if (label) {
    label.textContent = lightState.project_label || "unknown";
  }

  // Update session badge
  const badge = root.querySelector(".session-badge");
  const sessions = lightState.sessions || [];
  if (badge) {
    if (displayMode === "compact" && lights.length > 1) {
      badge.textContent = `${lights.length}`;
      badge.title = "点击打开项目列表";
      badge.classList.remove("hidden");
    } else if (shouldShowBadge(sessions)) {
      badge.textContent = getBadgeText(sessions);
      badge.title = "多会话";
      badge.classList.remove("hidden");
    } else {
      badge.classList.add("hidden");
    }
  }

  // Status to lamp mapping:
  // - Working: Green (AI is actively processing)
  // - Waiting: Yellow (needs user attention)
  // - Done/Idle: Red (session ended or waiting for first prompt)
  root.querySelector(".lamp.red")?.classList.toggle("on", lightState.status === "Done" || lightState.status === "Idle");
  root.querySelector(".lamp.yellow")?.classList.toggle("on", lightState.status === "Waiting");
  root.querySelector(".lamp.green")?.classList.toggle("on", lightState.status === "Working");

  updateOriginChip(root, origin);
  updateSessionChips(root, sessions);
  updateLampIcons(root, lightState);
}

function updateOriginChip(root, origin) {
  const chip = root.querySelector(".origin-chip");
  if (!chip) return;

  const key = String(origin || "local").toLowerCase();
  const labels = {
    local: "本",
    wsl: "W",
    ssh: "S",
    remote: "远",
  };
  chip.textContent = labels[key] || "本";
  chip.title = originLabel(origin);
  chip.classList.toggle("hidden", root.classList.contains("traffic-light--app"));
  chip.dataset.origin = key;
  chip.classList.remove("origin-local", "origin-wsl", "origin-ssh", "origin-remote");
  chip.classList.add(`origin-${key}`);
}

function originLabel(origin) {
  switch (String(origin || "local").toLowerCase()) {
    case "wsl":
      return "WSL";
    case "ssh":
      return "SSH";
    case "remote":
      return "远程";
    default:
      return "本地";
  }
}

function updateSessionChips(root, sessions) {
  const container = root.querySelector(".session-chips");
  if (!container) return;

  container.replaceChildren();
  if (root.classList.contains("traffic-light--app") || sessions.length <= 1) {
    container.hidden = true;
    container.classList.remove("is-interactive");
    return;
  }

  const isInteractive = displayMode === "parallel" && sessions.length > 1;
  container.classList.toggle("is-interactive", isInteractive);
  container.hidden = false;

  sessions.forEach((session) => {
    const chip = document.createElement("span");
    const meta = toolMeta(session.tool);
    chip.className = `tool-chip ${meta.chipClass}`;
    chip.title = `${meta.label} · ${session.task_name || session.session_id}`;
    chip.textContent = meta.icon;

    if (isInteractive) {
      chip.addEventListener("click", (event) => {
        event.stopPropagation();
        if (session.status === "Waiting" || session.status === "Done") {
          safeInvoke("confirm_session", { sessionId: session.session_id });
        }
      });
    }

    container.appendChild(chip);
  });
}

function createLamp(color, isOn) {
  const lamp = document.createElement("div");
  lamp.className = `lamp ${color}${isOn ? " on" : ""}`;
  const icon = document.createElement("span");
  icon.className = "lamp-icon";
  icon.hidden = true;
  lamp.appendChild(icon);
  return lamp;
}

function updateLampIcons(root, lightState) {
  const status = lightState.status;
  const sessions = lightState.sessions || [];
  const activeColor =
    status === "Working" ? "green" : status === "Waiting" ? "yellow" : "red";

  for (const color of ["red", "yellow", "green"]) {
    const lamp = root.querySelector(`.lamp.${color}`);
    const icon = lamp?.querySelector(".lamp-icon");
    if (!icon) continue;

    if (color !== activeColor || !lamp.classList.contains("on")) {
      icon.hidden = true;
      icon.textContent = "";
      icon.classList.remove("claude", "codex", "cursor");
      continue;
    }

    const matching = sessions
      .filter((session) => session.status === status)
      .sort(
        (left, right) =>
          statusPriority(right.status) - statusPriority(left.status),
      );
    const primary = matching[0] || sessions[0];
    if (!primary) {
      icon.hidden = true;
      icon.textContent = "";
      icon.classList.remove("claude", "codex", "cursor");
      continue;
    }

    const meta = toolMeta(primary.tool);
    icon.hidden = false;
    icon.textContent = meta.icon;
    icon.classList.remove("claude", "codex", "cursor");
    icon.classList.add(meta.chipClass);
  }
}

function toolMeta(tool) {
  switch (String(tool)) {
    case "Codex":
      return { label: "Codex", chipClass: "codex", icon: "◇" };
    case "Cursor":
      return { label: "Cursor", chipClass: "cursor", icon: "●" };
    default:
      return { label: "Claude", chipClass: "claude", icon: "◆" };
  }
}

function tooltipFor(lightState) {
  const origin = lightState.monitor_origin || lightState.monitorOrigin;
  const parts = [
    lightState.project_label || lightState.project_id,
    origin ? `来源: ${origin}` : null,
    lightState.status || "Idle",
  ].filter(Boolean);

  if (lightState.last_tool_call) {
    parts.push(lightState.last_tool_call);
  }

  return parts.join("\n");
}

function showMenu(x, y, items) {
  menu.replaceChildren();

  for (const [label, action, className] of [["关闭", hideMenu, "menu-close"], ...items]) {
    const item = document.createElement("button");
    item.type = "button";
    item.textContent = label;
    if (className) {
      item.classList.add(className);
    }
    item.addEventListener("click", () => {
      hideMenu();
      action();
    });
    menu.appendChild(item);
  }

  menu.hidden = false;
  const { innerWidth, innerHeight } = window;
  const rect = menu.getBoundingClientRect();
  menu.style.left = `${Math.max(
    MENU_EDGE_GUTTER,
    Math.min(x, innerWidth - rect.width - MENU_EDGE_GUTTER),
  )}px`;
  menu.style.top = `${Math.max(
    MENU_EDGE_GUTTER,
    Math.min(y, innerHeight - rect.height - MENU_EDGE_GUTTER),
  )}px`;
  scheduleWindowResize();
}

function hideMenu() {
  menu.hidden = true;
  scheduleWindowResize();
}

function scheduleWindowResize() {
  if (resizeFrame) {
    cancelAnimationFrame(resizeFrame);
  }

  resizeFrame = requestAnimationFrame(() => {
    requestAnimationFrame(resizeWindowToContent);
  });
}

async function resizeWindowToContent() {
  resizeFrame = 0;
  if (!currentWindow) return;

  const bodyStyle = getComputedStyle(document.body);
  const paddingY =
    parseFloat(bodyStyle.paddingTop) + parseFloat(bodyStyle.paddingBottom);

  const contentSize = measureVisibleContent();
  let width = Math.ceil(measureContentRightEdge() + WINDOW_GUTTER_X);
  let height = Math.ceil(contentSize.height + paddingY + WINDOW_GUTTER_Y);

  if (!menu.hidden) {
    const menuRect = menu.getBoundingClientRect();
    width = Math.max(width, Math.ceil(menuRect.right + MENU_EDGE_GUTTER));
    height = Math.max(height, Math.ceil(menuRect.bottom + MENU_EDGE_GUTTER));
  }

  if (!drawer.hidden) {
    const drawerPanel = drawer.querySelector(".session-drawer");
    const drawerRect = (drawerPanel || drawer).getBoundingClientRect();
    width = Math.ceil(
      drawerRect.right + MENU_EDGE_GUTTER + DRAWER_EXTRA_GUTTER,
    );
    height = Math.max(
      height,
      Math.ceil(drawerRect.bottom + MENU_EDGE_GUTTER),
    );
  }

  width = Math.max(72, width);
  height = Math.max(76, height);

  if (lastWindowSize.width === width && lastWindowSize.height === height) {
    return;
  }

  try {
    await tauriCore?.invoke("resize_main_window", { width, height });
    lastWindowSize = { width, height };
    return;
  } catch (error) {
    console.debug("resizeWindowToContent", error);
  }

  const LogicalSize = window.__TAURI__?.dpi?.LogicalSize;
  if (!LogicalSize) return;

  try {
    await currentWindow.setSize(new LogicalSize(width, height));
    lastWindowSize = { width, height };
  } catch (error) {
    console.debug("resizeWindowToContent fallback", error);
  }
}

function measureContentRightEdge() {
  let right = 0;
  for (const child of document.body.children) {
    if (child.hidden || child.id === "menu") continue;
    const rect = child.getBoundingClientRect();
    if (rect.width > 0) right = Math.max(right, rect.right);
  }
  return right;
}

function measureVisibleContent() {
  const children = [...container.children].filter((child) => !child.hidden);
  if (children.length === 0) {
    return { width: 0, height: 0, count: 0 };
  }

  const containerStyle = getComputedStyle(container);
  const gap = parseFloat(containerStyle.columnGap || containerStyle.gap) || 0;
  const width =
    children.reduce((sum, child) => sum + child.offsetWidth, 0) +
    gap * Math.max(0, children.length - 1);
  const height = Math.max(...children.map((child) => child.offsetHeight));

  return { width, height, count: children.length };
}

function shouldStartDrag(event) {
  if (event.button !== 0 || !menu.hidden) {
    return false;
  }

  if (
    event.target.closest(
      ".menu, button, #drawer, .session-drawer, .session-row, .drawer-close",
    )
  ) {
    return false;
  }

  // Standby handle: drag anywhere on the widget (left click never opens settings).
  if (event.target.closest(".traffic-light--app")) {
    return true;
  }

  // Project lights: drag from the whole widget; badge/chips keep click-only behavior.
  if (event.target.closest(".session-badge")) {
    return false;
  }

  if (
    displayMode === "parallel" &&
    event.target.closest(".tool-chip")
  ) {
    return false;
  }

  return Boolean(event.target.closest(".traffic-light:not(.traffic-light--app)"));
}

async function safeInvoke(command, payload) {
  try {
    return await tauriCore?.invoke(command, payload);
  } catch (error) {
    console.debug(command, error);
    return undefined;
  }
}

async function refreshLights() {
  const nextLights = await safeInvoke("get_lights");
  if (Array.isArray(nextLights)) {
    lights = nextLights;
    render();
  }
}

async function copyProjectPath(projectId) {
  const path = await safeInvoke("copy_path", { projectId });
  if (path && navigator.clipboard) {
    await navigator.clipboard.writeText(path);
  }
}

async function showDiagnostics() {
  const diagnostics = await safeInvoke("get_diagnostics");
  if (!diagnostics) return;

  const codexSessionPaths = Array.isArray(diagnostics.codex_sessions_paths)
    ? diagnostics.codex_sessions_paths
    : [diagnostics.codex_sessions_path].filter(Boolean);
  const codexManualPaths = Array.isArray(diagnostics.codex_manual_paths)
    ? diagnostics.codex_manual_paths
    : [];
  const codexMissingPaths = Array.isArray(diagnostics.codex_missing_paths)
    ? diagnostics.codex_missing_paths
    : [];

  const text = [
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

  if (navigator.clipboard) {
    await navigator.clipboard.writeText(text);
  }
  alert(text);
}

async function loadUiConfig() {
  const config = await safeInvoke("get_ui_config");
  if (config?.displayMode) {
    displayMode = config.displayMode;
  }
}

loadUiConfig().then(() => {
  refreshLights();
  scheduleWindowResize();
});
window.setInterval(refreshLights, 1000);

currentWindow
  ?.listen?.("tauri://move", async () => {
    try {
      const pos = await currentWindow.outerPosition();
      if (pos) {
        await safeInvoke("persist_window_position", { x: pos.x, y: pos.y });
      }
    } catch {}
  })
  .catch?.(() => {});
