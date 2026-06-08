# Building Deva Light

Last updated: 2026-06-06

Deva Light is a Tauri 2 desktop app with two Rust binaries:

- `deva-light`: the Tauri desktop app.
- `deva-light-hook`: the Claude Code hook helper bundled into the app and copied to `~/.deva_light/bin/` on startup.

## Prerequisites

### All Platforms

- Rust 1.70+ (install via [rustup](https://rustup.rs))
- Node.js 18+ and npm/pnpm

### Linux (Ubuntu/Debian) / WSL

WSL/Linux 需要安装以下系统依赖才能编译 Tauri：

```bash
sudo apt-get update
sudo apt-get install -y \
  pkg-config \
  libssl-dev \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev
```

### Windows

- Microsoft Visual Studio Build Tools (C++ toolchain)
- WebView2 (已内置于 Windows 10/11)

### macOS

- Xcode Command Line Tools: `xcode-select --install`

## Development

### Install Dependencies

```bash
# Node.js dependencies
npm ci

# Rust dependencies (自动通过 Cargo 安装)
cargo fetch --locked
```

### Run in Development Mode

```bash
npm run dev
```

This starts the Tauri dev server with hot reload.

## Remote Ubuntu -> Windows Mode

For the SSH workflow where Claude Code runs on Ubuntu and Deva Light displays on Windows, use the hook-only guide:

- [Ubuntu Hook-Only Forwarding](UBUNTU_HOOK_ONLY.md)

## Current Packaging Status

Windows packaging is verified.

Current Windows artifacts:

- `target/release/deva-light.exe`
- `target/release/bundle/msi/Deva Light_0.1.0_x64_en-US.msi`
- `target/release/bundle/nsis/Deva Light_0.1.0_x64-setup.exe`

macOS GUI packaging still needs validation. Ubuntu/Linux is hook-only for remote forwarding and does not ship a GUI package.

The main config currently targets the Windows hook binary:

```json
"resources": {
  "../target/release/deva-light-hook.exe": "deva-light-hook.exe"
}
```

For macOS, the bundled hook binary should be `deva-light-hook` without the `.exe` suffix.

## Windows Build

Run from the repository root on Windows:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo build -p deva-light-hook --release
npx @tauri-apps/cli@2.11.2 build
```

Expected artifacts:

```text
target/release/deva-light.exe
target/release/bundle/msi/Deva Light_0.1.0_x64_en-US.msi
target/release/bundle/nsis/Deva Light_0.1.0_x64-setup.exe
```

Smoke test:

```powershell
Start-Process -FilePath ".\target\release\deva-light.exe" -WindowStyle Hidden
Start-Sleep -Seconds 2
$runtime = Get-Content "$env:USERPROFILE\.deva_light\runtime.json" | ConvertFrom-Json
Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$($runtime.http_port)/health" |
  Select-Object -ExpandProperty Content
```

Expected output:

```text
ok
```

## macOS Build

Build on a macOS machine or macOS CI runner. macOS packaging should not be treated as buildable from Windows.

```bash
cargo build -p deva-light-hook --release
npx @tauri-apps/cli@2.11.2 build
```

Expected app binary:

```text
target/release/deva-light
```

Expected bundle outputs commonly include:

```text
target/release/bundle/macos/
target/release/bundle/dmg/
```

macOS notes:

- Local unsigned builds may work for personal testing.
- Public distribution needs Apple signing and notarization.
- Ensure the packaged app includes `deva-light-hook` as a resource.
- `.icns` is generated automatically from `icons/icon.png` during `cargo build`.

## Platform-Specific Resource Config

Windows GUI packaging is verified. macOS GUI packaging has a dedicated resource config:

macOS config:

```json
// src-tauri/tauri.macos.conf.json
{
  "bundle": {
    "resources": {
      "../target/release/deva-light-hook": "deva-light-hook"
    }
  }
}
```

Windows can keep:

```json
{
  "bundle": {
    "resources": {
      "../target/release/deva-light-hook.exe": "deva-light-hook.exe"
    }
  }
}
```

## Can Windows Build macOS?

Windows is suitable for building the Windows installer only.

macOS packages should be built on macOS because `.app`, `.dmg`, code signing, and notarization rely on Apple's toolchain.

macOS packaging from Windows is not a practical path.

## Recommended Release Path

Use CI runners per platform:

- Windows runner: build `deva-light-hook.exe`, then MSI/NSIS.
- macOS runner: build `deva-light-hook`, then `.app`/`.dmg`, with signing/notarization when ready.
- Ubuntu runner: optionally build/publish the hook-only `deva-light-hook` binary for remote forwarding.
