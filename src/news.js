const invoke = window.__TAURI__?.core?.invoke;
const tauriEvent = window.__TAURI__?.event;

const CATEGORIES = ["综合热榜", "科技", "社区", "国际", "财经", "其他", "收藏"];

let allSources = [];
let selectedOverride = loadSelectedOverride(); // null = 全选,否则 [id]
let favorites = loadFavorites();
let currentCategory = CATEGORIES[0];
const cache = Object.create(null); // sourceId -> NewsResult | { error }
const loading = new Set();

const listEl = document.getElementById("news-list");
const tabsEl = document.getElementById("category-tabs");
const statusEl = document.getElementById("news-status");
const refreshBtn = document.getElementById("refresh-news");
const manageBtn = document.getElementById("manage-sources");
const sourceManagerEl = document.getElementById("source-manager");
const sourceGroupsEl = document.getElementById("source-groups");
const closeSourceManagerBtn = document.getElementById("close-source-manager");

function loadSelectedOverride() {
  try {
    const raw = localStorage.getItem("news.selectedSources");
    if (raw === null) return null;
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function saveSelectedOverride(ids) {
  selectedOverride = ids;
  localStorage.setItem("news.selectedSources", JSON.stringify(ids ?? []));
}

function effectiveSelected() {
  if (selectedOverride && selectedOverride.length) return selectedOverride;
  return allSources.map((source) => source.id);
}

function loadFavorites() {
  try {
    const parsed = JSON.parse(localStorage.getItem("news.favorites") || "[]");
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function saveFavorites() {
  localStorage.setItem("news.favorites", JSON.stringify(favorites));
}

function favoriteKey(item) {
  return (item.url || item.title).trim();
}

function isFavorite(item) {
  const key = favoriteKey(item);
  return favorites.some((entry) => favoriteKey(entry) === key);
}

function toggleFavorite(item) {
  const key = favoriteKey(item);
  const index = favorites.findIndex((entry) => favoriteKey(entry) === key);
  if (index >= 0) {
    favorites.splice(index, 1);
  } else {
    favorites.push({ title: item.title, url: item.url || null });
    saveFavorites();
  }
  saveFavorites();
  if (currentCategory === "收藏") {
    render();
  } else {
    render(); // refresh star states in place
  }
}

async function init() {
  try {
    allSources = await invoke("get_news_sources");
  } catch (error) {
    statusEl.textContent = String(error);
    return;
  }
  renderTabs();
  renderSourceManager();
  await loadAll(false);
}

async function loadAll(force) {
  const ids = effectiveSelected();
  statusEl.textContent = force ? "刷新中…" : "加载中…";
  await Promise.all(ids.map((id) => loadSource(id, force)));
  renderStatus();
  render();
}

async function loadSource(id, force) {
  loading.add(id);
  try {
    const result = await invoke("fetch_news", { source: id, force: !!force });
    cache[id] = result;
  } catch (error) {
    cache[id] = { error: String(error), items: [] };
  } finally {
    loading.delete(id);
    renderStatus();
    render();
  }
}

function renderStatus() {
  if (loading.size > 0) {
    statusEl.textContent = `加载中…（${loading.size}）`;
    return;
  }
  const ids = effectiveSelected();
  const times = ids
    .map((id) => cache[id]?.updatedTime)
    .filter(Boolean)
    .sort((a, b) => b - a);
  if (times.length) {
    const date = new Date(times[0]);
    const hh = String(date.getHours()).padStart(2, "0");
    const mm = String(date.getMinutes()).padStart(2, "0");
    statusEl.textContent = `更新于 ${hh}:${mm}`;
  } else {
    statusEl.textContent = "";
  }
}

function renderTabs() {
  tabsEl.innerHTML = "";
  for (const category of CATEGORIES) {
    const btn = document.createElement("button");
    btn.textContent = category;
    btn.classList.toggle("active", category === currentCategory);
    btn.addEventListener("click", () => {
      currentCategory = category;
      renderTabs();
      render();
    });
    tabsEl.appendChild(btn);
  }
}

function render() {
  listEl.innerHTML = "";

  if (currentCategory === "收藏") {
    renderFavorites();
    return;
  }

  const selected = effectiveSelected();
  const sourcesInCategory = allSources.filter(
    (source) => source.category === currentCategory && selected.includes(source.id),
  );

  if (sourcesInCategory.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = "该分类下没有启用的资讯源，点右上角「资讯源」添加。";
    listEl.appendChild(empty);
    return;
  }

  for (const source of sourcesInCategory) {
    listEl.appendChild(renderSourceSection(source));
  }
}

function renderSourceSection(source) {
  const section = document.createElement("section");
  section.className = "source-section";

  const header = document.createElement("div");
  header.className = "source-name";
  header.textContent = source.name;
  const data = cache[source.id];
  if (!data || data.error) {
    header.insertAdjacentHTML(
      "beforeend",
      '<span class="err">加载失败</span>',
    );
  }
  section.appendChild(header);

  if (loading.has(source.id)) {
    section.appendChild(muted("加载中…"));
    return section;
  }
  if (!data) {
    return section;
  }
  if (data.error) {
    const msg = muted(data.error);
    section.appendChild(msg);
    return section;
  }

  const items = data.items || [];
  if (!items.length) {
    section.appendChild(muted("暂无内容"));
    return section;
  }
  for (const item of items) {
    section.appendChild(renderItem(item));
  }
  return section;
}

function renderItem(item) {
  const row = document.createElement("div");
  row.className = "news-item";

  const title = document.createElement("div");
  title.className = "news-title" + (item.url ? " link" : "");
  title.textContent = item.title;
  if (item.url) {
    title.title = item.url;
    title.addEventListener("click", () => {
      invoke("open_in_browser", { url: item.url }).catch((error) => {
        statusEl.textContent = String(error);
      });
    });
  }
  row.appendChild(title);

  const star = document.createElement("button");
  const on = isFavorite(item);
  star.className = "star" + (on ? " on" : "");
  star.textContent = on ? "★" : "☆";
  star.title = on ? "取消收藏" : "收藏";
  star.addEventListener("click", () => {
    toggleFavorite(item);
  });
  row.appendChild(star);

  return row;
}

function renderFavorites() {
  if (!favorites.length) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = "还没有收藏。点击资讯右侧的 ☆ 收藏，方便随时回看。";
    listEl.appendChild(empty);
    return;
  }
  const section = document.createElement("section");
  section.className = "source-section";
  for (const entry of favorites) {
    section.appendChild(
      renderItem({ title: entry.title, url: entry.url || undefined }),
    );
  }
  listEl.appendChild(section);
}

function muted(text) {
  const div = document.createElement("div");
  div.className = "muted";
  div.textContent = text;
  return div;
}

function renderSourceManager() {
  sourceGroupsEl.innerHTML = "";
  const selected = new Set(effectiveSelected());
  const groups = {};
  for (const source of allSources) {
    (groups[source.category] ||= []).push(source);
  }
  for (const category of CATEGORIES.filter((c) => c !== "收藏")) {
    const sources = groups[category] || [];
    if (!sources.length) continue;
    const title = document.createElement("div");
    title.className = "source-group-title";
    title.textContent = category;
    sourceGroupsEl.appendChild(title);
    for (const source of sources) {
      const label = document.createElement("label");
      label.className = "source-option";
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.checked = selected.has(source.id);
      checkbox.addEventListener("change", () => {
        const next = new Set(effectiveSelected());
        if (checkbox.checked) {
          next.add(source.id);
        } else {
          next.delete(source.id);
        }
        saveSelectedOverride(Array.from(next));
        loadAll(false);
      });
      label.appendChild(checkbox);
      const name = document.createElement("span");
      name.textContent = source.name;
      label.appendChild(name);
      sourceGroupsEl.appendChild(label);
    }
  }
}

refreshBtn.addEventListener("click", () => {
  void loadAll(true);
});

manageBtn.addEventListener("click", () => {
  renderSourceManager();
  sourceManagerEl.hidden = false;
});

closeSourceManagerBtn.addEventListener("click", () => {
  sourceManagerEl.hidden = true;
});

tauriEvent?.listen("news-reload", () => {
  void loadAll(false);
});

void init();
