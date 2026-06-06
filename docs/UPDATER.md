# Deva Light 自动更新

应用内置 Tauri Updater，新版本发布后会自动推送到已安装设备，在 **设置 → 关于** 中一键更新，无需重新下载安装包。

## 用户侧

1. 应用启动约 6 秒后会静默检查 GitHub Release 是否有新版本
2. 若有更新，会弹出系统通知
3. 打开 **设置 → 关于**，点击 **立即更新并重启** 即可

也可通过：

- 托盘图标 → **检查更新**
- 主窗口右键 → **检查更新**

## 发布侧（维护者）

### 一次性：配置 GitHub Secret

项目已在 `tauri.conf.json` 中写入**公钥**。私钥保存在本地 `.tauri/deva-light.key`（已 gitignore，切勿提交）。

将私钥内容写入 GitHub Actions Secret：

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY < .tauri/deva-light.key

# 若生成密钥时设置了密码（可选）
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

若本地密钥丢失，需重新 `npx tauri signer generate` 并更新公钥；**已安装用户将无法验证旧签名链**，需重新手动安装一次。

### 发布新版本

1. bump 版本号（三处保持一致）：
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
2. 提交并打 tag：

```bash
git tag v0.1.13
git push origin v0.1.13
```

3. GitHub Actions `Release` workflow 会构建安装包、签名 updater 产物、上传 `latest.json`，并**直接发布** Release。

### 更新源

```
https://github.com/wybyMrH/Deva_Light/releases/latest/download/latest.json
```

## 注意事项

- **首次**从无 updater 的旧版升级，仍需手动安装一次带 updater 的版本；之后均可应用内更新
- Release 必须是 **已发布** 状态（非 Draft），`latest.json` 才会对外可见
- 仓库需为 **公开**，否则未认证的 `latest.json` 请求会返回 404
- Windows 使用 NSIS `.exe` 作为更新包
- macOS 使用 `.app.tar.gz` 更新包
- 开发模式（`npm run dev`）不会自动检查更新

## 重新生成签名密钥

```bash
CI=1 npx @tauri-apps/cli@2 signer generate -w .tauri/deva-light.key -f --ci
```

将 `.tauri/deva-light.key.pub` 内容更新到 `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`。
