# Deva Light

**Deva Light** = Developer's Traffic Light，开发者的红绿灯。

桌面端 AI 编程助手状态监控工具，实时显示 Claude Code 和 Codex 的运行状态。

![Deva Light Screenshot](https://github.com/wybyMrH/Deva_Light/assets/screenshot.png)

---

## 功能特性

### 双监控支持

| 工具 | 监控方式 | 支持平台 |
|------|----------|----------|
| **Claude Code** | HTTP Hook 事件推送 | Windows / macOS / Linux |
| **Codex** | Sessions 文件监听 | Windows / macOS |

### 项目聚合灯组

- 每个项目独立显示一组三色灯
- 项目名显示在灯组顶部
- 自动识别项目：Git root + package.json / Cargo.toml / pyproject.toml / go.mod

### 会话抽屉

- 同一项目多会话时，灯组右上角显示会话数角标（如 `2/3`）
- 点击灯组展开右侧抽屉面板
- 会话按优先级排序：🟡 等待操作 > 🟢 工作中 > 🔴 已完成

### 跨平台支持

| 平台 | 支持方式 |
|------|----------|
| **Windows** | 完整 GUI，NSIS/MSI 安装包 |
| **macOS** | 完整 GUI，.app/.dmg 安装包 |
| **Ubuntu/Linux** | Hook-only 模式，转发到远程 GUI |

### WSL 远程开发

- WSL 中只安装 hook 二进制
- 事件转发到 Windows GUI
- 支持跨网络监控

---

## 状态说明

| 状态 | 颜色 | 含义 | 触发事件 |
|------|------|------|----------|
| **Working** | 🟢 绿色 | AI 正在工作中 | prompt-submit, pre-tool-use, post-tool-use, task_started |
| **Waiting** | 🟡 黄色 | 等待用户操作 | permission-request, notification, error |
| **Done** | 🔴 红色 | 任务已完成 | stop, task_complete |
| **Idle** | ⚫ 空闲 | 会话启动，等待首次提示 | session-start |

---

## 安装

### Windows

1. 从 [Releases](https://github.com/wybyMrH/Deva_Light/releases) 下载最新版本
2. 运行 `Deva Light_x64-setup.exe` 安装
3. 首次启动会提示安装 Claude Code hooks

### macOS

1. 从 [Releases](https://github.com/wybyMrH/Deva_Light/releases) 下载 `.dmg`
2. 拖拽到 Applications 文件夹
3. 首次运行可能需要在"系统偏好设置 > 安全性与隐私"中允许

### Ubuntu/Linux (Hook-only)

用于 SSH 远程开发场景，Ubuntu 只安装 hook 二进制转发事件到 Windows/macOS GUI。

```bash
# 下载并安装
curl -sL https://github.com/wybyMrH/Deva_Light/releases/latest/download/install-ubuntu-hook.sh | bash -s -- http://WINDOWS_IP:17321
```

参数说明：
- `WINDOWS_IP`: 运行 Deva Light GUI 的 Windows 主机 IP
- `17321`: 默认 HTTP 端口，可在设置中修改

---

## 使用方法

### 基本操作

1. **启动应用**：双击桌面图标或从开始菜单启动
2. **安装 Hooks**：首次启动会提示安装 Claude Code hooks，点击确认
3. **正常使用**：使用 Claude Code 或 Codex，灯组会自动显示状态
4. **确认状态**：点击黄灯或红灯灯组可确认并清除状态

### 右键菜单

右键点击灯组可打开菜单：
- **Open**：打开项目目录
- **Copy Path**：复制项目路径
- **Settings**：打开设置窗口
- **Remove**：移除灯组

### 设置

点击右上角 "DL" 图标打开设置：
- **HTTP Bind**：监听地址（默认 127.0.0.1，远程转发设为 0.0.0.0）
- **HTTP Port**：监听端口（默认随机分配）

---

## 开发

### 环境要求

- Rust 1.70+
- Node.js 18+
- pnpm / npm

### 本地运行

```bash
# 克隆仓库
git clone https://github.com/wybyMrH/Deva_Light.git
cd Deva_Light

# 安装依赖
npm install

# 开发模式
npm run dev

# 运行测试
cargo test

# 构建
npm run build
```

### 项目结构

```
Deva_Light/
├── src-tauri/          # Tauri Rust 后端
│   ├── src/
│   │   ├── main.rs     # 入口
│   │   ├── http_server.rs  # HTTP 事件接收
│   │   ├── aggregator.rs   # 状态聚合
│   │   ├── codex_watcher.rs # Codex 监听
│   │   ├── hook_installer.rs # Claude hooks 安装
│   │   ├── project.rs  # 项目识别
│   │   └── types.rs    # 类型定义
│   └── tests/          # 单元测试
├── src-hook/           # Hook 二进制
│   └── src/main.rs     # 从 stdin 读 JSON，POST 到 HTTP Server
├── src/                # WebView 前端
│   ├── index.html      # 主界面
│   ├── app.js          # 灯组渲染
│   ├── drawer.js       # 会话抽屉
│   └── styles.css      # 样式
├── scripts/            # 安装脚本
│   └── install-ubuntu-hook.sh
├── docs/               # 文档
│   ├── BUILDING.md     # 构建指南
│   └── UBUNTU_HOOK_ONLY.md
└── .github/workflows/  # CI/CD
    └── release.yml     # 自动发布
```

### 技术栈

- **后端**：Rust + Tauri 2
- **前端**：原生 HTML/CSS/JavaScript
- **通信**：HTTP Server + Tauri IPC
- **打包**：NSIS (Windows) / DMG (macOS)

---

## 工作原理

### Claude Code 监控

1. Deva Light 启动 HTTP Server（默认端口随机）
2. 安装 Claude Code hooks，将事件推送到 HTTP Server
3. 收到事件后更新灯组状态

### Codex 监控

1. 监听 `~/.codex/sessions` 目录下的 rollout JSONL 文件
2. 解析事件类型（task_started, task_complete 等）
3. 更新对应项目的灯组状态

### 状态聚合

1. 每个 session 有独立状态
2. 项目灯组显示聚合状态（优先级：Waiting > Working > Done/Idle）
3. 多会话时显示会话数角标和抽屉

---

## 常见问题

### Q: 灯组不显示？

1. 确认 Claude Code hooks 已安装（设置中查看状态）
2. 确认 HTTP Server 正在运行（检查日志）
3. 尝试手动触发事件

### Q: WSL 转发不工作？

1. 确认 Windows GUI 正在运行
2. 确认 HTTP Bind 设置为 `0.0.0.0`
3. 确认防火墙允许端口访问

### Q: 如何卸载？

1. Windows：通过"添加/删除程序"卸载
2. macOS：删除 Applications 中的 Deva Light.app
3. 清理 Claude hooks：设置中点击"移除 Hooks"

---

## 致谢

本项目参考了以下开源项目的设计思路：
- [ai-light](https://github.com/LeoKemp223/ai-light) - Tauri 跨平台架构
- [claude-code-traffic-light](https://github.com/nick1udwig/claude-code-traffic-light) - Hook 事件映射
- [codex-traffic-light](https://github.com/nick1udwig/codex-traffic-light) - 会话抽屉设计

---

## License

MIT License - 详见 [LICENSE](LICENSE) 文件
