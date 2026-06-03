# WSL 编译依赖安装指南

Deva Light 使用 Tauri 2 框架，在 Linux/WSL 上编译需要以下系统依赖。

## 一键安装（Ubuntu/Debian/WSL）

```bash
sudo apt-get update && sudo apt-get install -y \
  pkg-config \
  libssl-dev \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev
```

## 依赖说明

| 包名 | 用途 |
|------|------|
| `pkg-config` | 查找库路径 |
| `libssl-dev` | OpenSSL 开发头文件 |
| `libgtk-3-dev` | GTK 3 GUI 工具包 |
| `libwebkit2gtk-4.1-dev` | WebKitGTK WebView |
| `libappindicator3-dev` | 系统托盘支持 |
| `librsvg2-dev` | SVG 渲染 |
| `libsoup-3.0-dev` | HTTP 客户端/服务端库 |
| `libjavascriptcoregtk-4.1-dev` | JavaScript 引擎 |

## 验证安装

安装完成后运行：

```bash
pkg-config --modversion openssl
pkg-config --modversion gtk+-3.0
pkg-config --modversion webkit2gtk-4.1
```

如果都输出版本号，说明依赖已就绪。

## 然后编译

```bash
cd /path/to/Deva_Light
source "$HOME/.cargo/env"
cargo check
```
