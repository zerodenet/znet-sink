# Zero 适配层接口

本文档描述 GUI 前端应使用的 Rust 后端业务接口。前端不直接消费 Zero 原始 IPC JSON，也不自行构造 `query` / `command` 帧。

## 边界

- Rust 后端是 `GUI <-> zero` 的适配层。
- 前端调用 GUI 语义命令，Rust 后端负责转换为 Zero IPC。
- Rust 后端负责把 Zero 返回值归一化成稳定 DTO。
- `core_ipc_query`、`core_ipc_command`、`core_ipc_request` 只保留为专业模式诊断入口，不作为常规业务接口。
- Zero 未声明支持的能力，Rust 返回 `supported=false`、`enabled=false` 或带 `reason` 的不可操作状态，不伪造能力。

## 命令总览

| 命令 | 模式 | 说明 |
| --- | --- | --- |
| `gui_connect` | 简约/专业 | 一键连接：导出配置、启动 Zero、等待 health、开启系统代理 |
| `gui_disconnect` | 简约/专业 | 一键断开：关闭系统代理、停止 GUI 托管的 Zero |
| `gui_connection_status` | 简约/专业 | 获取连接聚合状态 |
| `gui_self_test_snapshot` | 简约/专业 | 获取自测准备状态与阻塞项 |
| `gui_proxy_mode_status` | 简约/专业 | 获取代理模式：全局、规则、直连 |
| `gui_set_proxy_mode` | 简约/专业 | 切换代理模式并同步 Zero active 配置 |
| `gui_core_overview` | 简约/专业 | 总览：进程、健康、统计、策略、能力 |
| `gui_core_health` | 简约/专业 | Zero 健康状态 |
| `gui_zero_capabilities` | 简约/专业 | Zero 能力声明的 GUI DTO |
| `gui_traffic_stats` | 简约/专业 | 流量与会话统计 |
| `gui_traffic_snapshot` | 简约/专业 | GUI 流量快照，含累计流量与实时速率 |
| `gui_policy_groups` | 简约/专业 | 策略组列表 |
| `gui_select_policy` | 简约/专业 | 切换策略组目标 |
| `gui_connections` | 专业 | 活动连接列表 |
| `gui_connection_detail` | 专业 | 单连接详情 |
| `gui_close_connection` | 专业 | 关闭连接 |
| `gui_dns_status` | 专业 | DNS 能力状态 |
| `gui_tun_status` | 专业 | TUN 能力状态 |
| `gui_rule_status` | 专业 | 规则/路由能力状态 |
| `gui_events_start` | 简约/专业 | 启动 GUI 归一化事件流 |
| `gui_events_stop` | 简约/专业 | 停止 GUI 归一化事件流 |

## 调用示例

```ts
import { invoke } from '@tauri-apps/api/core';

const status = await invoke('gui_connection_status');
const connected = await invoke('gui_connect');
const disconnected = await invoke('gui_disconnect');

const selfTest = await invoke('gui_self_test_snapshot');
const overview = await invoke('gui_core_overview');

const proxyMode = await invoke('gui_proxy_mode_status');
await invoke('gui_set_proxy_mode', {
  input: { mode: 'rule' }
});

await invoke('gui_select_policy', {
  policyTag: 'proxy',
  targetTag: 'direct'
});
```

## 一键连接

前端总览页应优先使用一键连接接口，不要自行串联配置导出、内核启动、health 检查和系统代理开关。

`gui_connect` 执行顺序：

1. 检查 active 代理配置。
2. 导出 active 配置为 Zero 启动配置。
3. 启动 GUI 托管的 Zero 进程；如果已运行则复用。
4. 等待 Zero IPC health 可用。
5. 开启系统代理到 `AppConfig.localProxy`。
6. 返回聚合连接状态。

`gui_disconnect` 执行顺序：

1. 关闭系统代理。
2. 停止 GUI 托管的 Zero 进程。
3. 返回聚合连接状态。

`gui_connection_status` 返回：

```json
{
  "connected": true,
  "stage": "connected",
  "process": {
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
    "exitReason": null,
    "lastError": null
  },
  "systemProxy": {
    "enabled": true,
    "host": "127.0.0.1",
    "port": 7890
  },
  "health": {
    "healthy": true,
    "engineVersion": "0.0.2",
    "startedAtUnixMs": 1713500000000
  },
  "stats": {
    "activeSessions": 0,
    "totalStarted": 0,
    "completedSessions": 0,
    "failedSessions": 0,
    "blockedSessions": 0,
    "directSessions": 0,
    "chainedSessions": 0,
    "bytesUp": 0,
    "bytesDown": 0
  },
  "activeProxyConfigId": "proxy-config-1",
  "localProxyHost": "127.0.0.1",
  "localProxyPort": 7890,
  "lastError": null
}
```

`stage` 常见值：

| stage | 说明 |
| --- | --- |
| `status` | 普通状态查询 |
| `connected` | 一键连接完成 |
| `disconnected` | 一键断开完成 |
| `failed` | 连接或断开流程失败 |

## 自测快照

阶段目标是自测可用时，前端或调试入口可以先调用 `gui_self_test_snapshot`，用它判断当前是否具备跑通一条最短链路的条件。

`gui_self_test_snapshot` 不会启动或停止 Zero，也不会修改系统代理；它只汇总当前状态：

```json
{
  "ready": true,
  "blockingIssues": 0,
  "warningCount": 2,
  "activeProxyConfigId": "proxy-config-1",
  "activeProxyConfigName": "default",
  "checks": [
    {
      "key": "coreLaunchConfig",
      "status": "pass",
      "message": "core launch config is ready",
      "details": {}
    }
  ],
  "suggestedFlow": [
    "proxy_config_import or subscription_sync",
    "proxy_config_set_active",
    "gui_proxy_mode_status",
    "gui_set_proxy_mode",
    "gui_connect",
    "gui_connection_status",
    "gui_disconnect"
  ]
}
```

`ready=true` 只表示没有阻塞项；如果 Zero 尚未运行，`coreHealth` 和 `systemProxy` 可能仍是 `warn`，这是自测开始前的正常状态。

## 代理模式

代理模式是 GUI 业务能力，不是前端直接修改 Zero 原始配置。Rust 后端负责把用户选择转换为当前 zero 内核可接受的 active proxy config 路由结构，然后持久化、导出给 Zero 使用。

`gui_proxy_mode_status` 返回：

```json
{
  "mode": "rule",
  "activeProxyConfigId": "proxy-config-1",
  "globalOutbound": "proxy",
  "ruleCount": 12,
  "hasRoute": true,
  "exported": false,
  "coreRunning": true,
  "restartedCore": false,
  "systemProxyEnabled": true,
  "requiresReconnect": false,
  "reason": null
}
```

`gui_set_proxy_mode` 入参：

```ts
await invoke('gui_set_proxy_mode', {
  input: {
    mode: 'global', // 'global' | 'rule' | 'direct'
    globalOutbound: 'proxy',
    restartCore: true
  }
});
```

转换规则：

| GUI 模式 | 当前 zero 0.0.3 active config |
| --- | --- |
| `global` | `mode = { "type": "global", "outbound": "<globalOutbound>" }` |
| `rule` | `mode = { "type": "rule" }`，并保留 `route.rules` 与既有 `route.final` |
| `direct` | `mode = { "type": "direct" }` |

注意：

- 切换模式不删除 `route.rules`、策略组或出站节点。
- `globalOutbound` 未提供时，Rust 会从当前配置中优先推导 `proxy`，再退化到第一个非 `direct` tag。
- 当前 zero 0.0.3 仍要求配置中存在 `route`；Rust 写入顶层 `mode` 时会保留或补齐 `route.final`。
- Rust 读取状态时兼容顶层 `mode`、未来可能出现的 `route.mode`，以及旧配置中的 `route.final`。
- `restartCore` 默认是 `true`。如果 Zero 已由 GUI 托管运行，后端会重新导出 active 配置并重启 Zero，使模式切换立即生效。
- 如果 `restartCore=false` 且 Zero 正在运行，返回 `requiresReconnect=true`，前端应引导用户重连。

## 总览

`gui_core_overview` 返回：

```json
{
  "process": {
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
    "exitReason": null,
    "lastError": null
  },
  "available": true,
  "health": {
    "healthy": true,
    "engineVersion": "0.0.2",
    "startedAtUnixMs": 1713500000000
  },
  "stats": {
    "activeSessions": 2,
    "totalStarted": 10,
    "completedSessions": 8,
    "failedSessions": 0,
    "blockedSessions": 0,
    "directSessions": 4,
    "chainedSessions": 4,
    "bytesUp": 1024,
    "bytesDown": 2048
  },
  "policyGroups": [],
  "capabilities": {
    "available": true,
    "apiVersion": "zero.api.v1",
    "schemaVersion": "zero.event.v1",
    "features": ["query", "runtime-snapshot", "flow-snapshot", "policy-snapshot"],
    "permissions": ["read"],
    "adapters": [],
    "sinks": [],
    "error": null
  },
  "lastError": null
}
```

## 流量快照

GUI 实时速率不使用 Zero `Push Connector`。Push Connector 属于节点主动上报到外部管理端点的远程集成能力，不是本地 GUI 的主控制链路。

前端应使用 `gui_traffic_snapshot` 获取 GUI 可直接展示的累计值和速率，不应直接读取 Zero 原始 `Stats` 并自行计算。Rust 后端会使用本地控制面 `Stats` 返回的 `bytes_up` / `bytes_down` 累计值，基于上一次采样计算 bps。

```ts
const traffic = await invoke('gui_traffic_snapshot');
```

返回：
```json
{
  "totals": {
    "activeSessions": 2,
    "totalStarted": 10,
    "completedSessions": 8,
    "failedSessions": 0,
    "blockedSessions": 0,
    "directSessions": 4,
    "chainedSessions": 4,
    "bytesUp": 1024,
    "bytesDown": 2048
  },
  "rates": {
    "uploadBps": 1024,
    "downloadBps": 2048
  },
  "sampledAtUnixMs": 1713500000000,
  "intervalMs": 1000,
  "source": "zero-stats",
  "stable": true,
  "reason": null
}
```

约定：

- 第一次采样没有上一个基线，`stable=false`，速率为 0。
- `totals.bytesUp` / `totals.bytesDown` 表示当前 zero 运行周期内的累计流量，不做按天、按月或全部历史统计。
- 短间隔重复调用会返回 `stable=false`，避免展示抖动的瞬时速率。
- 建议前端 1s 周期调用一次；如果已打开 `gui_events_start`，也可以用 `traffic.sampled` 事件补充图表。
- `gui_traffic_stats` 仅保留为累计统计的便捷接口，常规 GUI 展示速率应优先使用 `gui_traffic_snapshot`。

## 策略组

`gui_policy_groups` 返回 `GuiPolicyGroup[]`：

```json
[
  {
    "tag": "proxy",
    "kind": "selector",
    "selected": "direct",
    "members": [
      {
        "tag": "direct",
        "kind": null,
        "selected": true,
        "alive": true,
        "delayMs": null
      }
    ],
    "available": true,
    "reason": null
  }
]
```

`gui_select_policy` 入参：

```json
{
  "policyTag": "proxy",
  "targetTag": "direct"
}
```

返回：

```json
{
  "policyTag": "proxy",
  "targetTag": "direct",
  "selected": "direct",
  "accepted": true,
  "message": null
}
```

## 连接

`gui_connections` 入参：

```json
{
  "options": {
    "limit": 100,
    "inboundTag": "socks5",
    "principalKey": null
  }
}
```

返回：

```json
{
  "items": [
    {
      "flowId": "42",
      "network": "tcp",
      "source": null,
      "destination": "example.com:443",
      "inboundTag": "socks5",
      "outboundTag": "server-a",
      "policyTag": null,
      "routeMode": "rule",
      "outcome": "direct-relayed",
      "bytesUp": 1024,
      "bytesDown": 2048,
      "throughputUpBps": null,
      "throughputDownBps": null,
      "startedAtUnixMs": 1713500000000,
      "updatedAtUnixMs": null,
      "durationMs": null
    }
  ],
  "total": null,
  "limit": 100
}
```

## 能力状态

`gui_dns_status`、`gui_tun_status`、`gui_rule_status` 返回：

```json
{
  "key": "tun",
  "supported": false,
  "enabled": false,
  "state": "unsupported",
  "reason": "zero capability does not declare tun"
}
```

这些命令不直接表示产品目标是否存在，只表示当前 Zero 控制面是否声明了该能力。

## 交互面

`gui_interaction_surface_snapshot` 已合并：

- `uiMode`
- Zero `Capabilities.features`
- Rust 后端模式 guard

前端应优先根据该快照决定功能入口是否可见、可操作或只读。

## GUI 事件流

业务前端应订阅 GUI 归一化事件，而不是 Zero 原始事件。

启动事件流：

```ts
const sub = await invoke('gui_events_start', {
  events: undefined,
  options: undefined
});

await listen(sub.eventName, (event) => {
  const payload = event.payload;
});

await listen(sub.statusEventName, (event) => {
  const status = event.payload;
});
```

停止事件流：

```ts
await invoke('gui_events_stop');
```

`gui_events_start` 返回：

```json
{
  "generation": 1,
  "eventName": "gui:event",
  "statusEventName": "gui:event-status"
}
```

`gui:event-status`：

```json
{
  "generation": 1,
  "status": "subscribed",
  "error": null
}
```

`status` 可为：

| status | 说明 |
| --- | --- |
| `subscribed` | 已订阅 Zero 事件流 |
| `disconnected` | 事件流自然断开 |
| `stopped` | 被 `gui_events_stop` 或新 generation 停止 |
| `offline` | Zero IPC 不可用 |
| `error` | 其他错误 |

`gui:event`：

```json
{
  "generation": 1,
  "event": {
    "eventType": "connection.updated",
    "sourceEventType": "flow.updated",
    "eventId": "flow.updated:42:1713500010000",
    "sequence": 1024,
    "occurredAtUnixMs": 1713500010000,
    "payload": {
      "kind": "connection",
      "data": {
        "flowId": "42",
        "network": "tcp",
        "destination": "example.com:443",
        "inboundTag": "socks5",
        "outboundTag": "server-a",
        "bytesUp": 1024000,
        "bytesDown": 5120000,
        "throughputUpBps": 8192,
        "throughputDownBps": 32768
      }
    }
  }
}
```

事件类型映射：

| Zero 事件 | GUI 事件 |
| --- | --- |
| `engine.started` | `core.statusChanged` |
| `engine.stopped` | `core.statusChanged` |
| `engine.warning` | `core.warning` |
| `flow.started` | `connection.started` |
| `flow.updated` | `connection.updated` |
| `flow.completed` | `connection.closed` |
| `policy.selected` | `policy.selected` |
| `stats.sampled` | `traffic.sampled` |

未知事件会被转换为 `core.unknownEvent`，用于内部日志或诊断，不应直接驱动用户界面。
