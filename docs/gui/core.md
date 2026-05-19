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

Windows 下 `socket = null` 表示使用 zero 默认 named pipe：`\\.\pipe\zero-control`。

非 Windows 下 `socket = null` 时，Rust 会把 socket 解析为 zero 可执行文件同目录的 `zero-control.sock`，并在 `core_process_start` 时追加：

```text
zero run --control-socket <zero-dir>/zero-control.sock <configPath>
```

如果用户在 `AppConfig.core.socket` 中显式配置路径，则所有平台都使用该路径。

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

进程托管不依赖 IPC。

## IPC 控制面

IPC 只面向运行中的内核。GUI 没启动内核或内核未运行时，IPC 命令可能返回 `core_unavailable`。

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
