# 内核接入

内核接入分三层：

- GUI 应用层：内核路径、配置路径、socket、启动参数解析。
- 进程托管：通过 CLI 启动/停止 GUI 托管的 zero 进程。
- IPC 控制面：只面向已经运行中的内核。

## 内核配置快照

| 命令 | 说明 |
| --- | --- |
| `core_config_get` | 获取解析后的内核配置快照 |
| `core_config_export_active` | 将 active 代理配置写出为 zero 配置文件，并更新 `AppConfig.core.configPath` |

## 调用参数

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `core_config_get` | 无 | `CoreConfigSnapshot` |
| `core_config_export_active` | 无 | `CoreConfigExportResult` |

`core_config_get` 返回：

```json
{
  "kernel": "zero",
  "autoConnect": true,
  "autoStart": false,
  "executablePath": "C:\\...\\build\\core\\zero.exe",
  "executableExists": true,
  "configPath": "C:\\...\\zero-active-config.json",
  "configExists": true,
  "workingDir": "C:\\...\\build\\core",
  "workingDirExists": true,
  "socket": null,
  "endpoint": {
    "transport": "named-pipe",
    "path": "\\\\.\\pipe\\zero-control"
  },
  "launchArgs": ["run", "C:\\...\\zero-active-config.json"],
  "warnings": []
}
```

`core_config_export_active` 返回：

```json
{
  "proxyConfigId": "proxy-config-1",
  "path": "C:\\...\\zero-active-config.json",
  "appConfig": {
    "kernel": "zero",
    "autoConnect": true,
    "autoStart": false,
    "executablePath": "C:\\...\\build\\core\\zero.exe",
    "executableExists": true,
    "configPath": "C:\\...\\zero-active-config.json",
    "configExists": true,
    "workingDir": "C:\\...\\build\\core",
    "workingDirExists": true,
    "socket": null,
    "endpoint": {
      "transport": "named-pipe",
      "path": "\\\\.\\pipe\\zero-control"
    },
    "launchArgs": ["run", "C:\\...\\zero-active-config.json"],
    "warnings": []
  }
}
```

`appConfig` 是导出后的 `CoreConfigSnapshot`。

## 平台约定

GUI 为每个客户端进程生成独立 IPC 地址，并显式传给内核：Unix 使用 `zero-control-<GUI PID>.sock`，Windows 使用带 GUI PID 的 named pipe。显式 socket 配置保留原路径作为诊断覆盖；界面返回的 `endpoint` 和 `launchArgs` 是实际启动依据。

```text
zero run --parent-lifetime-stdin --control-socket <private-endpoint> <configPath>
```

客户端持有子进程 stdin 写端。正常退出会先清理托管 TUN 和系统代理，再停止子进程；客户端意外退出时 stdin EOF 也会触发支持该参数的内核退出。客户端只管理自身启动的子进程，不接管或终止历史遗留实例。

下面的 JSON 仅示意字段结构；实际 endpoint 包含客户端 PID。

## 进程托管

| 命令 | 说明 |
| --- | --- |
| `core_process_status` | 查询 GUI 托管的内核进程状态 |
| `core_process_start` | 使用 `zero run ...` 启动内核 |
| `core_process_stop` | 停止 GUI 托管启动的内核进程 |

调用参数：

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `core_process_status` | 无 | `CoreProcessStatus` |
| `core_process_start` | 无 | `CoreProcessStatus` |
| `core_process_stop` | 无 | `CoreProcessStatus` |

状态结构：

```json
{
  "state": "running",
  "pid": 1234,
  "kernel": "zero",
  "executablePath": "C:\\...\\zero.exe",
  "workingDir": "C:\\...\\build\\core",
  "configPath": "C:\\...\\zero-active-config.json",
  "endpointPath": "\\\\.\\pipe\\zero-control",
  "startedAtUnixMs": 1713500000000,
  "exitedAtUnixMs": null,
  "exitCode": null,
  "lastError": null
}
```

`state` 可为：

| state | 说明 |
| --- | --- |
| `notstarted` | 未由 GUI 启动 |
| `starting` | 正在启动 |
| `running` | 运行中 |
| `exited` | 已退出 |
| `failed` | 启动、停止或轮询失败 |

启动成功要求私有 IPC 的 `health.healthy = true`，且 `runtime.pid` 等于本次启动的子进程 PID。后端最多等待 15 秒，并要求连续健康响应至少 300 毫秒，以捕获监听初始化阶段的立即退出。超时或提前退出会清理本次子进程并返回失败。这个检查证明控制面及进程存活；连接系统代理前另行等待本地代理端口可连接。

连接、断开、启动和升级共用配置操作锁。状态轮询在操作进行中不会撤销刻意保留的系统代理。

首次无代理配置启动依赖支持 management-only 待命的内核；本次配套内核已移除空配置启动的 `NoInbounds` 限制。旧内核可能通过配置校验却在运行时退出，不能用短暂 IPC 响应判断成功。

## 内核升级事务

下载、校验和、版本及生命周期参数兼容性检查、现有配置验证均在停止当前内核前完成。替换前保留旧文件和应用配置，随后停止自身托管进程、原子替换文件、更新路径；原先运行时，重新启动并确认 IPC 就绪，恢复原有 TUN 意图及托管系统代理后才提交成功。原先停止时保持停止。

失败时先停止候选进程，再恢复旧文件、旧应用配置和运行状态。自动恢复不完整会保留备份并返回 `kernel_upgrade_failed`，其详情包含 `rollbackRestored`、`rollbackErrors` 和 `backupPath`；无法恢复运行状态时尝试解除自身托管系统代理，避免继续指向不可用端口。备份及强制中断边界见 [本地存储](./storage.md)。

版本管理界面同时订阅 `kernel:download-progress` 和 `kernel:install-progress`。后者为 `{ version, stage }`，阶段包括 `preparing`、`validating`、`backing_up`、`installing`、`starting`、`rolling_back`。下载 100% 只表示下载完成，直到安装命令返回才允许关闭或重试；失败消息保留在窗口中。命令成功后前端仅刷新状态，不能再次保存可执行文件路径触发第二次运行时切换。

稳定版更新提示按语义版本比较，旧稳定版不会被提示为较新 RC 的升级。同版本重装仍替换可执行文件并修复 Unix 执行权限；回退也恢复原文件权限，即使文件内容没有变化。

业务前端通常不应直接串联 `core_config_export_active`、`core_process_start`、`system_proxy_enable`。总览页的一键连接/断开应使用 [Zero 适配层接口](./zero-adapter.md) 中的 `gui_connect`、`gui_disconnect` 和 `gui_connection_status`。

## IPC 控制面

IPC 只面向运行中的内核。GUI 没启动内核或内核未运行时，IPC 命令可能返回 `core_unavailable`。

业务前端不应直接使用低层 IPC 命令构造 Zero 原始请求。常规页面应使用 [Zero 适配层接口](./zero-adapter.md) 中的 `gui_*` 命令，由 Rust 后端负责把 Zero 原始返回转换为稳定 DTO。

`core_ipc_query`、`core_ipc_command`、`core_ipc_request` 保留为专业模式诊断入口，不作为常规业务接口。

## 本地 GUI 统计

GUI 的实时流量统计不使用 Zero `Push Connector`。Push Connector 属于节点主动上报外部管理端点的远程集成通道，适合远程面板、监控系统或少量远程命令，不作为本地 GUI 统计链路。

本地 GUI 应通过 Rust 后端的 [Zero 适配层接口](./zero-adapter.md) 使用 `gui_traffic_snapshot`。Rust 后端从本地控制面 `Stats` 获取累计值，并负责计算 `uploadBps` / `downloadBps`，前端只负责展示。

| 命令 | 内核请求 | 说明 |
| --- | --- | --- |
| `core_ipc_default_endpoint` | - | 当前配置解析后的 IPC endpoint |
| `core_status` | `ping` | 内核在线状态 |
| `core_ipc_ping` | `ping` | ping |
| `core_get_capabilities` | query `Capabilities` | 内核能力 |
| `core_get_health` | query `Health` | 内核健康 |
| `core_get_config` | query `Config` | 内核配置快照 |
| `core_get_runtime` | query `Runtime` | 运行时状态 |
| `core_get_stats` | query `Stats` | 统计 |
| `core_get_policies` | query `Policies` | 策略组 |
| `core_select_policy` | command `policies.select` | 切换 selector |
| `core_probe_policy` | command `policies.probe` | 触发 urltest |
| `core_close_flow` | command `flows.close` | 关闭连接 |
| `core_validate_config` | command `config.validate` | 校验配置，不改变运行状态 |
| `core_ipc_query` | query custom | 低层 query |
| `core_ipc_command` | command custom | 低层 command |
| `core_ipc_request` | raw frame | 原始 IPC 请求 |

调用参数：

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `core_ipc_default_endpoint` | 无 | `CoreEndpoint` |
| `core_status` | `{ options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_ipc_ping` | `{ options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_ipc_query` | `{ request, options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_ipc_command` | `{ method, params?: unknown, options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_ipc_request` | `{ frame, options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_get_capabilities` | `{ options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_get_health` | `{ options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_get_config` | `{ options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_get_runtime` | `{ options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_get_stats` | `{ options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_get_policies` | `{ options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_select_policy` | `{ policyTag, targetTag, options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_probe_policy` | `{ policyTag, options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_close_flow` | `{ flowId, options?: CoreIpcOptions }` | `CoreCallResult` |
| `core_validate_config` | `{ config, options?: CoreIpcOptions }` | `CoreCallResult` |

`CoreIpcOptions`：

```json
{
  "socket": null,
  "timeoutMs": 2000
}
```

不传 `socket` 时，Rust 使用应用配置解析出的 endpoint。

`CoreCallResult`：

```json
{
  "available": true,
  "endpoint": {
    "transport": "named-pipe",
    "path": "\\\\.\\pipe\\zero-control"
  },
  "response": {},
  "error": null
}
```

## 事件订阅

| 命令 | 说明 |
| --- | --- |
| `core_events_start` | 启动内核事件订阅 |
| `core_events_stop` | 停止当前事件订阅 generation |

`core_events_start` 转发 Zero 原始事件，仅作为专业模式诊断入口。业务前端应使用 [Zero 适配层接口](./zero-adapter.md) 中的 `gui_events_start`，接收 Rust 归一化后的 `gui:event`。

调用参数：

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `core_events_start` | `{ events?: string[], options?: CoreIpcOptions }` | `CoreEventSubscription` |
| `core_events_stop` | 无 | 新的 generation number |

`core_events_start` 返回：

```json
{
  "generation": 1,
  "eventName": "core:event",
  "statusEventName": "core:event-status"
}
```

前端监听：

- `core:event`
- `core:event-status`

`core:event-status` 状态包括：

| status | 说明 |
| --- | --- |
| `subscribed` | 订阅成功 |
| `disconnected` | 连接自然断开 |
| `stopped` | 被新的 generation 停止 |
| `offline` | 内核不可用 |
| `error` | 其他错误 |
