/**
 * Session Drawer Component
 *
 * Shows individual session status when a project has multiple sessions.
 * Drawer expands on the right side of the light group.
 *
 * Sorting: Waiting (yellow) > Working (red) > Done (green)
 */

const DRAWER_AUTO_CLOSE_DELAY = 3000; // 3 seconds
let drawerAutoCloseTimer = null;
let currentDrawerProjectId = null;

/**
 * Create drawer element for a project
 */
export function createDrawer() {
  const drawer = document.createElement("div");
  drawer.className = "session-drawer";
  drawer.hidden = true;

  const header = document.createElement("div");
  header.className = "drawer-header";

  const title = document.createElement("span");
  title.className = "drawer-title";
  title.textContent = "Sessions";

  const closeBtn = document.createElement("button");
  closeBtn.className = "drawer-close";
  closeBtn.type = "button";
  closeBtn.textContent = "×";
  closeBtn.addEventListener("click", () => hideDrawer());

  header.append(title, closeBtn);
  drawer.appendChild(header);

  const content = document.createElement("div");
  content.className = "drawer-content";
  drawer.appendChild(content);

  return drawer;
}

/**
 * Update drawer content with sessions
 */
export function updateDrawer(drawer, sessions) {
  const content = drawer.querySelector(".drawer-content");
  if (!content) return;

  content.replaceChildren();

  // Sort sessions: Waiting > Working > Done/Idle
  const sorted = [...sessions].sort((a, b) => {
    const priority = { Waiting: 0, Working: 1, Done: 2, Idle: 3 };
    return (priority[a.status] || 99) - (priority[b.status] || 99);
  });

  for (const session of sorted) {
    const row = createSessionRow(session);
    content.appendChild(row);
  }

  // Update drawer visibility based on session count
  const shouldShow = sessions.length > 1;
  if (shouldShow && drawer.hidden) {
    drawer.hidden = false;
    startAutoCloseTimer();
  } else if (!shouldShow && !drawer.hidden) {
    drawer.hidden = true;
    stopAutoCloseTimer();
  }
}

/**
 * Create a session row element
 */
function createSessionRow(session) {
  const row = document.createElement("div");
  row.className = "session-row";
  row.dataset.sessionId = session.session_id;
  row.dataset.status = session.status;

  // Status indicator
  const indicator = document.createElement("div");
  indicator.className = `session-status status-${session.status.toLowerCase()}`;
  indicator.textContent = statusIcon(session.status);

  // Session info
  const info = document.createElement("div");
  info.className = "session-info";

  const name = document.createElement("span");
  name.className = "session-name";
  name.textContent = session.task_name || session.session_id;

  const tool = document.createElement("span");
  tool.className = "session-tool";
  tool.textContent = session.tool === "ClaudeCode" ? "Claude" : "Codex";

  info.append(name, tool);

  // Click handler for actionable statuses
  if (session.status === "Waiting" || session.status === "Done") {
    row.classList.add("is-actionable");
    row.addEventListener("click", () => {
      const sessionId = row.dataset.sessionId;
      // Will call IPC to confirm session
      if (window.__TAURI__?.core) {
        window.__TAURI__.core.invoke("confirm_session", { sessionId });
      }
    });
  }

  row.append(indicator, info);
  return row;
}

/**
 * Get status icon for display
 */
function statusIcon(status) {
  switch (status) {
    case "Working":
      return "🟢";
    case "Waiting":
      return "🟡";
    case "Done":
      return "🔴";
    case "Idle":
      return "⚫";
    default:
      return "⚪";
  }
}

/**
 * Show drawer for a specific project
 */
export function showDrawer(projectId, drawer) {
  currentDrawerProjectId = projectId;
  drawer.hidden = false;
  startAutoCloseTimer();
}

/**
 * Hide drawer
 */
export function hideDrawer() {
  const drawer = document.querySelector(".session-drawer");
  if (drawer) {
    drawer.hidden = true;
  }
  currentDrawerProjectId = null;
  stopAutoCloseTimer();
}

/**
 * Start auto-close timer
 */
function startAutoCloseTimer() {
  stopAutoCloseTimer();
  drawerAutoCloseTimer = setTimeout(() => {
    hideDrawer();
  }, DRAWER_AUTO_CLOSE_DELAY);
}

/**
 * Stop auto-close timer
 */
function stopAutoCloseTimer() {
  if (drawerAutoCloseTimer) {
    clearTimeout(drawerAutoCloseTimer);
    drawerAutoCloseTimer = null;
  }
}

/**
 * Get badge text for session count
 */
export function getBadgeText(sessions) {
  if (sessions.length <= 1) return "";
  const completed = sessions.filter(s => s.status === "Done").length;
  return `${completed}/${sessions.length}`;
}

/**
 * Check if should show badge
 */
export function shouldShowBadge(sessions) {
  return sessions.length > 1;
}