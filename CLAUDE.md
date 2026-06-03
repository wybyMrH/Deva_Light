# Deva Light

桌面端 AI 编程助手红绿灯，同时监控 Claude Code 和 Codex 的运行状态。

**Deva Light** = Developer's Traffic Light，开发者的红绿灯。

## 功能特性

- **双监控**：同时支持 Claude Code 和 Codex
- **项目聚合灯组**：按项目显示灯组，项目名在顶部
- **会话抽屉**：同一项目多会话时，展开右侧面板显示每个会话状态
- **WSL→Windows 转发**：WSL 开发时 hook 转发到 Windows GUI
- **跨平台**：Windows/macOS GUI，Ubuntu hook-only 模式

## 状态说明

- 🟢 绿灯：AI 正在工作中
- 🟡 黄灯：等待用户操作（权限请求、通知）
- 🔴 红灯：任务已完成或会话空闲

## 开发

### 环境要求

- Rust 1.70+
- Node.js 18+
- pnpm/npm

### 本地运行

```bash
# 安装依赖
npm install

# 开发模式
npm run dev

# 构建
npm run build
```

### 测试

```bash
cargo test
```

## 项目结构

```
src-tauri/       # Tauri Rust 后端
src-hook/        # Hook 二进制
src/             # WebView 前端
scripts/         # 安装脚本
docs/            # 文档
```

## License

MIT
