# ZNet Sink

<!-- 🚧 TODO: 添加截图或演示 GIF
<p align="center">
  <img src="docs/assets/screenshot.png" alt="ZNet Sink 截图" width="800" />
</p>
-->

<p align="center">
  <strong>零域网络代理桌面客户端</strong> — 跨平台、高性能、开箱即用
</p>

<p align="center">
  <a href="https://github.com/zerodenet/znet-sink/releases"><img src="https://img.shields.io/github/v/release/zerodenet/znet-sink?include_prereleases" alt="Release" /></a>
  <a href="https://github.com/zerodenet/znet-sink/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MPL--2.0-blue" alt="License: MPL-2.0" /></a>
  <a href="https://github.com/zerodenet/znet-sink/actions/workflows/release.yml"><img src="https://github.com/zerodenet/znet-sink/actions/workflows/release.yml/badge.svg" alt="CI" /></a>
  <img src="https://img.shields.io/badge/rust-1.75+-orange" alt="Rust 1.75+" />
  <img src="https://img.shields.io/badge/node-20+-green" alt="Node 20+" />
</p>

---

## 简介

ZNet Sink 是 [ZeroDenet](https://github.com/zerodenet) 零域网络的官方桌面代理客户端，基于 **Tauri 2** 构建，使用 Rust 驱动系统级网络代理，前端采用 SvelteKit 提供现代化操作界面。

核心定位：**一个 GUI 管理面**，负责托管 Zero 内核进程、管理代理配置、订阅同步、规则集分发和实时流量监控。

## 平台支持

| 平台 | 架构 | 状态 |
|------|------|------|
| Windows 10/11 | x86_64 | ✅ 完全支持 |
| macOS | Intel · Apple Silicon | ✅ 完全支持 |
| Linux | x86_64 | ✅ 完全支持 |

安装包格式：Windows NSIS/MSI · macOS DMG · Linux deb/AppImage。所有平台均支持**自动更新**。

## ✨ 功能特性

- 🚀 **一键连接 / 断开** — 从总览页即可完成内核启动、配置写入、系统代理设置的全流程
- 📊 **实时流量监控** — Rust 后端从内核 Stats 接口采集累计值，前端展示上行 / 下行速率
- 📦 **代理配置管理** — 支持多套代理配置切换、导入导出；active 配置一键写为内核可读格式
- 🔄 **订阅同步** — 远程托管配置拉取、自动同步、流量信息展示
- 🧭 **规则集管理** — 路由规则、分流策略的本地管理
- 🎛️ **简约 / 专业双模式** — 简约模式隐藏高级配置入口，专业模式开放诊断工具和原始 IPC 控制面
- 🔌 **系统代理集成** — 连接时自动设置系统代理、断开时自动恢复
- 📋 **应用日志与内核日志** — 双层日志面板，支持选择性过滤
- 🛡️ **本地优先** — 所有数据（配置、订阅、规则集）持久化在本地数据目录，不上传
- 🌐 **Zero IPC 控制面** — ping / query / command / subscribe 完整协议支持，专业模式可手动构造帧

## 📥 安装

从 [Releases](https://github.com/zerodenet/znet-sink/releases) 页面下载对应平台的安装包。

> **注意**：Zero 内核二进制文件需要放置在 `build/core/` 目录下（或通过应用配置指定自定义路径）。Windows 下内核名为 `zero.exe`，其他平台为 `zero`。

## 🏗️ 技术架构

```
┌─────────────────────────────────────────────┐
│               SvelteKit SPA                  │
│   TypeScript 6 · Tailwind CSS 4 · shadcn/   │
│   invoke()  ────  Tauri IPC  ────  events   │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────▼──────────────────────────┐
│              Rust Backend (gui_lib)           │
│                                              │
│  commands/  →  services/  →  models/         │
│   (薄层)       (业务逻辑)     (数据结构)       │
│                                              │
│     core/ipc.rs  ────┐                      │
│     events/          │                      │
│     state/AppState   │                      │
│     (Mutex<RwLock>)  │                      │
└──────────────────────┼──────────────────────┘
                       │ JSON-line frames
┌──────────────────────▼──────────────────────┐
│          Zero 内核 (外部进程)                 │
│   named pipe (Win) / Unix socket (mac/Linux) │
└─────────────────────────────────────────────┘
```

### 前端 (`src/`)

- **SvelteKit SPA**（`adapter-static`，无 SSR）
- **Svelte 5 runes** 响应式语法 (`$state` / `$derived` / `$effect`)
- **Tailwind CSS 4** + shadcn-svelte 组件库
- 通过 `@tauri-apps/api/core` 的 `invoke()` 调用 Rust 命令
- 监听 Tauri 事件流获取内核实时推送

### Rust 后端 (`src-tauri/src/`)

| 层 | 模块 | 职责 |
|----|------|------|
| **Commands** | `commands/` | Tauri `#[command]` 处理函数 — 薄层，委托给 services |
| **Services** | `services/` | 业务逻辑、状态变更、持久化 |
| **Models** | `models/` | 可序列化数据结构 |
| **IPC** | `core/ipc.rs` | JSON-line 协议通过命名管道 (Windows) 或 Unix socket |
| **Events** | `events/` | 向 Svelte 前端推送内核事件的 Tauri 事件发射器 |
| **State** | `state/` | `AppState` — 所有应用数据，受 `Mutex` 保护 |

详见 [GUI 接入契约](docs/gui/README.md) 了解前端-后端交互边界，以及 [内核接入](docs/gui/core.md) 了解 IPC 协议细节。

## 🚀 开发指南

### 前置要求

| 依赖 | 最低版本 |
|------|----------|
| Node.js | 20+ |
| Rust | 1.75+ |
| pnpm | 9+ |

### 快速开始

```bash
# 克隆仓库
git clone https://github.com/zerodenet/znet-sink.git
cd znet-sink

# 安装前端依赖
pnpm install

# 仅前端开发 (http://localhost:1420)
pnpm dev

# 完整 Tauri 开发 (Rust + 前端)
pnpm tauri dev
```

### 常用命令

```bash
# 类型检查
pnpm check

# 构建前端
pnpm build

# 构建 Tauri 安装包
pnpm tauri build

# Rust 测试
cd src-tauri && cargo test
```

### 项目结构

```
gui/
├── src/                          # SvelteKit 前端
│   ├── lib/
│   │   ├── components/           # 应用组件
│   │   │   ├── ui/               # shadcn-svelte 基础组件
│   │   │   ├── core/             # 内核功能组件
│   │   │   ├── settings/         # 设置相关组件
│   │   │   ├── stats/            # 统计展示组件
│   │   │   └── tabs/             # 标签页组件
│   │   ├── services/             # invoke() 封装与响应式 store
│   │   ├── types/                # TypeScript 类型定义
│   │   └── constants/            # 导航、标签页常量
│   ├── routes/                   # SvelteKit 路由
│   └── app.css                   # Tailwind CSS 入口
├── src-tauri/                    # Rust 后端
│   ├── src/
│   │   ├── commands/             # 17 个命令模块 (~40 个 Tauri 命令)
│   │   ├── services/             # 业务服务层
│   │   ├── models/               # 序列化数据模型
│   │   ├── core/                 # Zero IPC 协议实现
│   │   ├── events/               # 事件发射器
│   │   └── state/                # 应用状态管理
│   ├── icons/                    # 多平台应用图标
│   └── tests/                    # Rust 集成测试
├── docs/                         # 开发文档
│   └── gui/                      # 前后端交互契约
└── .github/workflows/            # CI/CD (多平台构建 + 自动更新)
```

## 📝 提交规范

项目采用 [约定式提交](COMMIT_CONVENTION.md) (Conventional Commits)，通过 `commitlint` 和 `commitizen` 工具链强制执行。

```bash
# 交互式提交（推荐，自动生成规范格式）
pnpm commit

# 手动提交
git commit -m "feat(proxy): add config import validation"
git commit -m "fix(ipc): handle named pipe disconnect gracefully"
```

提交类型一览：[COMMIT_CONVENTION.md](COMMIT_CONVENTION.md)

## 🤝 参与贡献

欢迎提交 Issue 和 Pull Request！参与前请了解：

- [提交规范](COMMIT_CONVENTION.md) — 统一的 Conventional Commit 格式
- [GUI 接入契约](docs/gui/README.md) — 前端-后端交互边界与调用约定
- [内核接入](docs/gui/core.md) — Zero 内核 IPC 协议文档
- [Nodes Manual QA](docs/gui/nodes-manual-qa.md) — 节点页与批量测速验收清单
- [Worktree Commit Plan](docs/gui/worktree-commit-plan.md) — 当前工作树分组提交计划

### 开发工具推荐

- [VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## 📄 许可证

本项目采用 [Mozilla Public License 2.0](https://www.mozilla.org/MPL/2.0/) 授权。

---

<p align="center">
  <a href="https://github.com/zerodenet">ZeroDenet</a> · <a href="https://github.com/zerodenet/znet-sink">GitHub</a>
</p>
