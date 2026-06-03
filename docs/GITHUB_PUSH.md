# GitHub 推送指南

由于 WSL 没有安装 GitHub CLI，请按以下步骤手动推送：

## 方法一：在 Windows 上操作

1. 打开 Windows PowerShell 或 Git Bash
2. 进入项目目录：
   ```powershell
   cd C:\Users\alice\projects\demo
   ```

3. 创建 GitHub 私有仓库并推送：
   ```powershell
   # 安装 GitHub CLI（如果没有）
   # winget install GitHub.cli

   # 登录 GitHub
   gh auth login

   # 创建私有仓库并推送
   gh repo create deva-light --private --source=. --push
   ```

## 方法二：手动创建仓库

1. 访问 https://github.com/new
2. 仓库名：`deva-light`
3. 选择 **Private**
4. 不要勾选 "Add a README file"（已有内容）
5. 点击 "Create repository"

6. 在 Windows 上推送：
   ```powershell
   cd C:\Users\alice\projects\demo
   git remote add origin https://github.com/wybyMrH/deva-light.git
   git branch -M main
   git push -u origin main
   ```

## 推送后

仓库地址：https://github.com/wybyMrH/deva-light
