# Gesture Interactive Desktop 🖐️

基于 **Tauri v2** + **Vue 3** + **MediaPipe Hands** 的手势交互桌面应用。通过摄像头实时捕捉手部动作，实现无接触式桌面操控。

## ✨ 功能特点

- **实时手势识别** — 基于 TensorFlow.js + MediaPipe Hands 的高精度手部关键点检测
- **无接触操控** — 用手势替代鼠标操作
- **桌面应用** — 基于 Tauri v2 构建，原生 Windows 体验，低资源占用

## 🖥️ 技术栈

| 层级 | 技术 |
|------|------|
| 前端框架 | Vue 3 + TypeScript |
| 构建工具 | Vite 6 |
| 桌面框架 | Tauri v2 (Rust) |
| 手势识别 | TensorFlow.js + MediaPipe Hands |
| 包管理 | pnpm |

## 📦 安装

### 下载安装包

从 [Releases](https://github.com/BaldwinShu/Gesture-Interactive-Desktop/releases) 下载最新版本：

- `my-desktop-app_0.1.0_x64-setup.exe` — 安装包（推荐）
- `my-desktop-app_0.1.0_x64_en-US.msi` — Windows Installer 包

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/BaldwinShu/Gesture-Interactive-Desktop.git
cd Gesture-Interactive-Desktop

# 安装依赖
pnpm install

# 开发模式运行
pnpm tauri dev

# 构建可执行文件
pnpm tauri build
```

> **注意**：首次运行 `pnpm tauri dev` 或 `pnpm tauri build` 时，Rust 需要编译依赖，耗时较长。

## 🚀 使用说明

1. 启动应用后，允许摄像头权限
2. 将手置于摄像头前，系统会自动识别手势
3. 支持的手势操作包括：
   - 手势控制鼠标移动
   - 抓取/点击操作

## 🛠️ 开发

```bash
# 前端开发（浏览器预览）
pnpm dev

# 前端构建
pnpm build

# Tauri 命令
pnpm tauri build    # 构建桌面安装包
```

### 项目结构

```
Gesture-Interactive-Desktop/
├── src/                  # Vue 前端源码
│   ├── App.vue          # 主应用组件（手势识别 + 交互逻辑）
│   ├── main.ts          # 入口文件
│   └── composables/     # 组合式函数
├── src-tauri/            # Rust 后端
│   ├── src/
│   │   ├── main.rs      # Tauri 入口
│   │   └── lib.rs       # 库逻辑
│   ├── Cargo.toml       # Rust 依赖配置
│   └── tauri.conf.json  # Tauri 应用配置
├── public/mediapipe/    # MediaPipe 手势识别模型
└── package.json         # Node.js 依赖配置
```

## 📄 许可证

本项目基于 MIT 许可证开源。
