# GUI 接入契约

本文档描述前端与 `src-tauri` 后端之间的交互契约。当前 GUI 只对接自研内核 `zero`，前端不直接解析内核配置、不直接判断业务能力；这些事实由 Rust 侧给出。

## 边界

- Rust 负责数据层、能力层、配置解析、订阅同步、内核 IPC、进程托管、文件持久化。
- 前端负责交互行为和展示，包括菜单布局、卡片展示、禁用态、跳转流程。
- 菜单和操作显隐来自 `gui_interaction_surface_snapshot`。
- `gui_capabilities_snapshot` 仅用于内部诊断，不作为用户菜单入口。
- 用户偏好的显隐来自 `app_config_get().ui.hiddenMenuKeys`。
- 当前阶段以自测可用为目标，可以先用 `gui_self_test_snapshot` 定位阻塞项，再跑 `gui_connect` / `gui_set_proxy_mode` / `gui_disconnect`。

## 通用调用约定

前端通过 Tauri `invoke` 调用命令：

```ts
import { invoke } from '@tauri-apps/api/core';

const config = await invoke('app_config_get');
```

所有命令返回 `Result<T, AppError>`。错误结构：

```json
{
  "code": "invalid_argument",
  "message": "human readable message",
  "details": null
}
```

常见错误码：

| code | 含义 |
| --- | --- |
| `invalid_argument` | 参数或配置内容无效 |
| `not_found` | 指定资源不存在 |
| `io_error` | 文件或系统 IO 失败 |
| `upstream_error` | 订阅远端拉取失败 |
| `core_unavailable` | 内核 IPC 不可用 |
| `timeout` | 内核 IPC 超时 |
| `connection_closed` | 内核 IPC 连接关闭 |
| `core_error` | 内核运行时返回错误 |
| `internal` | GUI 后端内部错误 |

## 文档索引

- [应用配置](./app-config.md)
- [交互模式约束](./interaction-modes.md)
- [能力快照](./capabilities.md)
- [Zero 适配层接口](./zero-adapter.md)
- [内核接入](./core.md)
- [代理配置](./proxy-config.md)
- [订阅管理](./subscriptions.md)
- [规则集配置](./rule-sets.md)
- [日志](./logs.md)
- [Nodes Manual QA](./nodes-manual-qa.md)
- [Worktree Commit Plan](./worktree-commit-plan.md)
