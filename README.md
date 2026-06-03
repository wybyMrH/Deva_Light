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

## 安装

### Windows

下载最新的安装包：`Deva Light_x64-setup.exe`

### macOS

下载最新的 `.dmg` 安装包。

### Ubuntu hook-only

用于 SSH 远程开发场景，Ubuntu 只安装 hook 二进制转发事件到 Windows/macOS GUI。

```bash
./scripts/install-ubuntu-hook.sh http://WINDOWS_IP:17321
```

## 开发

```bash
# 安装依赖
npm install

# 开发模式
npm run dev

# 构建
npm run build

# 测试
cargo test
```

## License

MIT
