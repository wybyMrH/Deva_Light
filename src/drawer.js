/**
 * Session Drawer Component
 *
 * Shows individual session status when a project has multiple sessions.
 * Drawer expands to the right of the light group.
 *
 * Sorting: Waiting (yellow) > Working (green) > Done/Idle (red)
 */

let currentDrawerProjectId = null;
let drawerMode = "sessions";

/**
 * Create drawer element for a project
 */
export function createDrawer() {
  const drawer = document.createElement("div");
  drawer.className = "session-drawer";

  const header = document.createElement("div");
  header.className = "drawer-header";

  const title = document.createElement("span");
  title.className = "drawer-title";
  title.textContent = "会话列表";

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
export function updateDrawer(drawerRoot, sessions, projectLabel = "") {
  drawerMode = "sessions";
  const panel = drawerRoot.querySelector(".session-drawer");
  const content = drawerRoot.querySelector(".drawer-content");
  const title = drawerRoot.querySelector(".drawer-title");
  if (!content || !panel) return;

  if (title) {
    title.textContent = projectLabel ? `${projectLabel} · 会话` : "会话列表";
  }

  content.replaceChildren();

  const sorted = [...sessions].sort((a, b) => {
    const priority = { Waiting: 0, Working: 1, Done: 2, Idle: 3 };
    return (priority[a.status] || 99) - (priority[b.status] || 99);
  });

  for (const session of sorted) {
    content.appendChild(createSessionRow(session));
  }

  const shouldShow = sessions.length > 1;
  const wasHidden = drawerRoot.hidden;
  drawerRoot.hidden = !shouldShow;
  panel.hidden = !shouldShow;

  if (shouldShow && wasHidden) {
    notifyVisibilityChange();
  } else if (!shouldShow && !wasHidden) {
    currentDrawerProjectId = null;
    notifyVisibilityChange();
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

  const indicator = document.createElement("div");
  indicator.className = `session-status status-${session.status.toLowerCase()}`;
  indicator.textContent = statusIcon(session.status);

  const info = document.createElement("div");
  info.className = "session-info";

  const name = document.createElement("span");
  name.className = "session-name";
  name.textContent = session.task_name || shortenSessionId(session.session_id);

  const tool = document.createElement("span");
  tool.className = "session-tool";
  tool.textContent = session.tool === "ClaudeCode" ? "Claude" : "Codex";

  info.append(name, tool);

  if (session.status === "Waiting" || session.status === "Done") {
    row.classList.add("is-actionable");
    row.addEventListener("click", (event) => {
      event.stopPropagation();
      if (window.__TAURI__?.core) {
        window.__TAURI__.core.invoke("confirm_session", {
          sessionId: row.dataset.sessionId,
        });
      }
    });
  }

  row.append(indicator, info);
  return row;
}

function shortenSessionId(sessionId) {
  if (!sessionId || sessionId.length <= 16) {
    return sessionId || "unknown";
  }

  return `${sessionId.slice(0, 8)}…${sessionId.slice(-4)}`;
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
export function updateProjectDrawer(drawerRoot, projects) {
  drawerMode = "projects";
  const panel = drawerRoot.querySelector(".session-drawer");
  const content = drawerRoot.querySelector(".drawer-content");
  const title = drawerRoot.querySelector(".drawer-title");
  if (!content || !panel) return;

  if (title) {
    title.textContent = `项目列表 (${projects.length})`;
  }

  content.replaceChildren();

  const sorted = [...projects].sort((a, b) => {
    const priority = { Waiting: 0, Working: 1, Done: 2, Idle: 3 };
    return (priority[a.status] || 99) - (priority[b.status] || 99);
  });

  for (const project of sorted) {
    content.appendChild(createProjectRow(project));
  }
}

function createProjectRow(project) {
  const row = document.createElement("div");
  row.className = "session-row project-row";
  row.dataset.projectId = project.project_id;
  row.dataset.status = project.status;

  const indicator = document.createElement("div");
  indicator.className = `session-status status-${project.status.toLowerCase()}`;
  indicator.textContent = statusIcon(project.status);

  const info = document.createElement("div");
  info.className = "session-info";

  const name = document.createElement("span");
  name.className = "session-name";
  name.textContent = project.project_label || project.project_id;

  const tool = document.createElement("span");
  tool.className = "session-tool";
  tool.textContent = `${project.sessions?.length || 0} 个会话`;

  info.append(name, tool);
  row.append(indicator, info);
  row.addEventListener("click", (event) => {
    event.stopPropagation();
    window.dispatchEvent(
      new CustomEvent("drawer-project-selected", {
        detail: { projectId: project.project_id },
      }),
    );
  });

  return row;
}

export function showProjectDrawer(drawerRoot, projects) {
  drawerMode = "projects";
  currentDrawerProjectId = "__projects__";
  drawerRoot.hidden = false;

  const panel = drawerRoot.querySelector(".session-drawer");
  if (panel) {
    panel.hidden = false;
  }

  updateProjectDrawer(drawerRoot, projects);
  notifyVisibilityChange();
}

export function showDrawer(projectId, drawerRoot, projectLabel = "") {
  drawerMode = "sessions";
  currentDrawerProjectId = projectId;
  drawerRoot.hidden = false;

  const panel = drawerRoot.querySelector(".session-drawer");
  if (panel) {
    panel.hidden = false;
  }

  const title = drawerRoot.querySelector(".drawer-title");
  if (title && projectLabel) {
    title.textContent = `${projectLabel} · 会话`;
  }

  notifyVisibilityChange();
}

/**
 * Hide drawer
 */
export function hideDrawer() {
  const drawerRoot = document.getElementById("drawer");
  if (!drawerRoot) return;

  drawerRoot.hidden = true;
  const panel = drawerRoot.querySelector(".session-drawer");
  if (panel) {
    panel.hidden = true;
  }

  currentDrawerProjectId = null;
  drawerMode = "sessions";
  notifyVisibilityChange();
}

export function getDrawerMode() {
  return drawerMode;
}

export function isDrawerOpen() {
  const drawerRoot = document.getElementById("drawer");
  return Boolean(drawerRoot && !drawerRoot.hidden);
}

export function getCurrentDrawerProjectId() {
  return currentDrawerProjectId;
}

function notifyVisibilityChange() {
  window.dispatchEvent(new CustomEvent("drawer-visibility-changed"));
}

/**
 * Get badge text for session count
 */
export function getBadgeText(sessions) {
  if (sessions.length <= 1) return "";
  const completed = sessions.filter((s) => s.status === "Done").length;
  return `${completed}/${sessions.length}`;
}

/**
 * Check if should show badge
 */
export function shouldShowBadge(sessions) {
  return sessions.length > 1;
}
