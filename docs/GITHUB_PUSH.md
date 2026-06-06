# Git 推送指南

## 日常推送

```bash
cd /path/to/Deva_Light
git push origin main
```

## 发布新版本

```bash
git tag v0.1.12
git push origin v0.1.12
```

推送 tag 后会自动触发 [Release workflow](https://github.com/wybyMrH/Deva_Light/actions)。

## 使用 GitHub CLI（可选）

```bash
gh auth login
git push origin main
```

仓库地址：https://github.com/wybyMrH/Deva_Light
