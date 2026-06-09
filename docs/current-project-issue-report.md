# 当前项目实际问题报告

生成时间：2026-06-08  
范围：`C:\Users\higanbana\develop\rust\gui` 当前代码与最新 Zero 文档的实际偏差。

## 高优先级问题

### 0. 当前生命周期不符合“GUI 托管内核常驻”原则

位置：`src-tauri/src/lib.rs`、`src-tauri/src/models/app_config.rs`、`src-tauri/src/services/core_process.rs`

新的工作原则是 GUI 启动后立即检查内核，GUI 运行期间内核默认常驻，GUI 退出时托管内核退出；用户不允许普通关闭内核，只允许重启。当前实现仍是：

- `AppCoreConfig.auto_start` 默认 `false`
- Tauri `setup` 仅在 `auto_start=true` 时尝试启动内核
- `core_process_stop` 和 `gui_disconnect` 会停止内核
- 无配置时 `core_process::start` 直接失败

影响：默认启动后不保证内核可用，首次无配置状态无法进入“内核已运行但等待配置”的清晰状态。

建议：默认启动托管内核；普通 UI 移除“关闭内核”语义，改为“重启内核”；退出应用时保留托管内核清理。

### 1. IPC QueryResponse 没有按变体 key 解包

位置：`src-tauri/src/kernel/zero/parsing.rs`、`src-tauri/src/kernel/zero/queries.rs`

最新文档规定 IPC Query 响应为 `result: {"health": {...}}`、`result: {"runtime": {...}}`。当前 `unwrap_core_envelope()` 只返回 `result`，后续解析器多数直接读内部字段：

- `parse_health()` 不会进入 `health`，可能返回 `healthy=true` 但丢失 `engine_build_id`
- `parse_capabilities()` 不会进入 `capabilities`，features/adapters/sinks 可能为空
- `parse_connection_list()` 没有识别 `active_flows`
- `parse_connection()` 不会进入 `flow`
- `parse_feature_runtime_status()` 不会进入 `tun_status`

建议：按 query 变体统一解包，保留旧 shape 兼容。

### 2. Unix 默认 socket 路径与最新文档不一致

位置：`src-tauri/src/kernel/transport.rs`、`src-tauri/src/services/core_config.rs`、`docs/gui/core.md`

最新文档默认 Unix 路径是 `~/.zero/control.sock`。当前实现将 Unix socket 推导为 zero 可执行文件同目录 `zero-control.sock`，并在启动参数中加入 `--control-socket`。这对 GUI 托管进程可控，但不适合连接用户手动启动的默认 Zero daemon。

建议：区分外部默认 daemon endpoint 和 GUI 托管 endpoint，文档同步说明。

### 3. 代理模式模型仍使用 GUI 私有 `route.mode`

位置：`src-tauri/src/services/proxy_mode.rs`、`src-tauri/src/services/core_config.rs`

最新内核配置使用顶层 `mode`，运行时可用 `mode.set` 热切换。当前项目写入 `route.mode`，导出时又移除它并改写 `route.final`，运行中默认通过重启内核生效。

风险：

- 与最新配置模型不一致
- 不能使用内核原子模式切换
- 用户连接会因重启中断
- 项目文档和实际内核文档继续分叉

建议：优先实现 `mode.set`，未运行时写顶层 `mode`，旧 `route.mode` 只做兼容读取。

### 3.5 无配置文件时无法启动内核

位置：`src-tauri/src/services/core_process.rs`

当前 `start()` 在 `snapshot.config_path.is_none()` 时直接返回错误。按照新原则，GUI 启动后应尽量保持内核常驻，即便没有配置文件也应让内核以“等待配置/缺少配置”的状态运行，至少启用 `127.0.0.1:9090` Web API 控制面用于 health、capabilities 和提示。

建议：

- 优先推动内核支持无配置启动并返回明确状态。
- 短期由 GUI 生成最小临时配置，包含空 inbounds/outbounds 和 `api.control.enabled=true`、`listen=127.0.0.1:9090`。
- UI 展示“内核已运行，缺少代理配置”，而不是把它归类为启动失败。

## 中优先级问题

### 3.6 普通业务接口仍允许停止内核

位置：`src-tauri/src/commands/core_process.rs`、`src-tauri/src/services/gui_connection.rs`

在新模式下，内核是 GUI 的下层常驻服务，不应被用户普通关闭。当前 `core_process_stop`、`gui_disconnect` 仍会停止托管内核。`gui_disconnect` 未来应更明确：如果语义是“断开系统代理”，它不应停止内核；如果语义是“退出/停用托管服务”，则不应放在常规连接开关中。

建议：业务层拆分为：

- 断开代理：关闭系统代理，但保持内核运行。
- 重启内核：停止并立即重新启动内核。
- 退出 GUI：停止托管内核并清理系统代理。

### 4. 缺少部分最新控制面命令

当前缺少 GUI 业务接口或底层封装：

- `config.apply`
- `mode.set`
- `diagnostics.dns_lookup`
- `diagnostics.trace_route`
- `recent_flows`
- `sinks`

影响：配置热加载、路由解释、DNS 调试、事件投递状态和最近连接列表无法完整落地。

### 5. 能力发现丢弃协议矩阵

位置：`src-tauri/src/models/gui_core.rs`、`src/lib/types/gui-api.ts`

最新 `capabilities` 包含 `protocols` 和 `build_features`。当前 GUI DTO 只保留 `features`、`permissions`、`adapters`、`sinks`，无法表达：

- 协议 `supported` / `partial` / `experimental`
- inbound/outbound TCP/UDP 支持
- MUX 状态
- limitation codes

建议：扩展 Rust 和 TypeScript DTO，能力页按协议矩阵驱动。

### 6. 事件断流后没有统一自动重连策略

位置：`src-tauri/src/services/gui_events.rs`、`src/lib/services/core-events.svelte.ts`

当前订阅可成功转发事件，订阅成功后也会补拉 stats/runtime。但连接断开后主要标记为 `disconnected` 或 `offline`，没有在服务层统一执行 1 到 5 秒退避重连。

建议：后端或前端服务层实现自动重连；重连成功后补拉 `runtime`、`stats`、`policies` 对账。

### 7. TUN 状态查询存在非文档 fallback

位置：`src-tauri/src/kernel/zero/queries.rs`

最新文档是 query `{"tun_status":{}}` 和 command `tun.start` / `tun.stop`。当前失败后 fallback 到 command `tun.status`，这不是最新文档中的 command。更重要的是，在 QueryResponse 变体未解包时，`tun_status` 本身也可能解析不到 `running/name/addr/tag`。

建议：先修 `tun_status` 解包，再决定是否保留 `tun.status` 作为旧内核兼容。

## 低优先级问题

### 8. 前端 `queryFlows()` 构造了错误的 raw query 形态

位置：`src/lib/services/core.ts`

`queryFlows()` 调用 raw IPC 时传入：

```ts
request: { type: 'active_flows', limit: 100, filter: {} }
```

最新文档要求：

```json
{"active_flows":{"limit":100,"filter":{}}}
```

虽然常规业务连接页应使用 `gui_connections`，但该 raw helper 仍可能误导专业模式诊断。

### 9. 项目内部文档已经落后

位置：`docs/gui/core.md`、`docs/gui/app-config.md`

内部文档仍描述 Unix socket 为 zero 可执行文件同目录 `zero-control.sock`，这与最新内核文档默认 `~/.zero/control.sock` 不一致。`docs/gui/zero-adapter.md` 对事件和能力的描述也缺少最新 `capabilities.protocols` 和 `config.apply` 闭环。

建议：在实现修复后同步更新 GUI 文档，明确“GUI 托管路径”和“Zero 默认 daemon 路径”的差异。

## 建议立即验证

1. 用一个模拟 IPC 响应测试 `{"ok":true,"result":{"health":{...}}}` 是否能解析出版本号。
2. 测试 `{"ok":true,"result":{"active_flows":[...]}}` 是否能填充连接列表。
3. 测试 `{"ok":true,"result":{"tun_status":{"running":true,...}}}` 是否能显示 TUN running。
4. 在 Unix 环境下分别测试外部默认 Zero daemon 和 GUI 托管 Zero 的 endpoint 解析。
5. 对 `capabilities.protocols` 做快照测试，确保前端能看到协议限制。

## 当前未处理事项

本次只生成报告，没有修改业务代码，也没有运行 `pnpm check` 或 Rust 测试。工作区已有未提交变更：

- `src-tauri/tauri.conf.json`
- `src/lib/services/updater.svelte.ts`

这些变更未被本次报告任务触碰。
