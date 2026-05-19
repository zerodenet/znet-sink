# ZNet Sink

零域网络代理客户端 - 基于 Tauri + SvelteKit 构建的跨平台桌面应用

## ✨ 特性

- 🚀 高性能 Rust 后端 + 现代 Web 前端
- 🎨 Tailwind CSS 4 + shadcn-svelte 组件库
- 🔌 核心进程管理与 IPC 通信
- 📊 实时流量监控与日志面板
- ⚙️ 完整的应用配置管理
- 📦 订阅管理与规则集配置

## 🛠️ 技术栈

| 层级 | 技术 |
|------|------|
| **前端** | SvelteKit 5 + TypeScript 6 |
| **样式** | Tailwind CSS 4 + shadcn-svelte |
| **桌面框架** | Tauri 2 |
| **后端** | Rust |
| **包管理** | pnpm |

## 📋 前置要求

- Node.js 20+
- Rust 1.75+
- pnpm 9+

## 🚀 快速开始

### 安装依赖

```bash
pnpm install
```

### 开发模式

```bash
# 仅前端开发 (http://localhost:1420)
pnpm dev

# 完整 Tauri 开发环境 (Rust + 前端)
pnpm tauri dev
```

### 类型检查

```bash
pnpm check
```

## 📦 构建

```bash
# 构建前端
pnpm build

# 构建完整 Tauri 应用
pnpm tauri build
```

## 📝 提交规范

项目采用 [约定式提交规范](COMMIT_CONVENTION.md)：

```bash
# 交互式提交 (推荐)
pnpm commit

# 手动提交
git commit -m "feat: add new feature"
```

## 🏗️ 项目结构

```
gui/
├── src/                    # SvelteKit 前端
│   ├── lib/
│   │   ├── components/    # 组件
│   │   │   ├── ui/        # shadcn-svelte 组件
│   │   │   ├── core/      # 核心功能组件
│   │   │   └── tabs/      # 标签页组件
│   │   ├── services/      # 业务逻辑
│   │   ├── types/         # TypeScript 类型
│   │   └── utils.ts       # 工具函数
│   └── routes/            # 路由
├── src-tauri/             # Rust 后端
│   ├── src/
│   │   ├── commands/      # Tauri 命令
│   │   ├── models/        # 数据模型
│   │   ├── services/      # 服务层
│   │   ├── core/          # 核心 IPC
│   │   ├── events/        # 事件系统
│   │   └── state/         # 状态管理
│   └── tests/             # Rust 测试
└── docs/                  # 文档
```

## 🔧 开发工具推荐

- [VS Code](https://code.visualstudio.com/)
- [Svelte 扩展](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode)
- [Tauri 扩展](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer 扩展](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## 📄 许可证

MIT
