# 内核最新文档差异实现报告

生成时间：2026-06-08  
文档来源：`http://127.0.0.1:5173`，本地源目录 `C:\Users\higanbana\develop\rust\zero\docs`

## 范围

本报告对照 Zero 最新控制面文档与当前 GUI 项目实现，重点覆盖 IPC/HTTP 控制面、Query/Command/Event 契约、能力发现、配置模型和 GUI 接入流程。

## 总体结论

当前项目已经具备 Zero IPC 基础接入、GUI 归一化事件流、策略切换、流量统计、连接列表、TUN 启停等核心能力。但它更像是“可用的 GUI 适配层”，还没有完整按最新内核文档落地：IPC QueryResponse 外层变体解析不完整，Unix 默认 socket 路径与最新文档不一致，配置模式仍使用 GUI 私有 `route.mode` 并通过重启生效，能力矩阵没有暴露 `protocols`，部分命令和查询仍缺失。

## 新工作模式原则

后续 GUI 与内核应采用“GUI 托管、内核常驻”的工作模式：

- GUI 启动后第一时间检查内核运行情况。
- GUI 运行期间，内核作为 GUI 下层服务默认常驻运行。
- 用户不应关闭内核；GUI 可以提供“重启内核”，但不提供普通“关闭内核”作为业务操作。
- GUI 退出时，托管内核必须随 GUI 一起退出，避免残留后台进程。
- 没有配置文件时，GUI 仍应尽量启动内核，并默认启用 `127.0.0.1:9090` Web API 控制面，保证基础健康检查、能力发现和提示能力可用。
- 更理想的目标是：无配置文件时内核可启动并返回明确的“缺少配置/等待配置”状态；有配置文件时 GUI 自动保持内核常驻。

该原则会改变当前“可选 autoStart + 手动连接”的产品边界：`autoStart` 应逐步退化为兼容项或高级设置，默认行为应是 GUI 启动即托管内核。

## 差异矩阵

| 文档要求 | 当前实现 | 状态 | 影响 |
| --- | --- | --- | --- |
| GUI 推荐 IPC，Windows `\\.\pipe\zero-control`，Unix `~/.zero/control.sock` | Windows 默认正确；Unix 默认解析为 zero 可执行文件同目录 `zero-control.sock`，并启动时显式传 `--control-socket` | 部分符合 | GUI 托管进程可用，但连接外部默认 Zero daemon 时不符合文档路径 |
| 首屏顺序：`health` -> `capabilities` -> `config` -> `runtime` -> `subscribe` | `gui_core_overview` 查询 capabilities/health/stats/policies；事件订阅成功后前端补拉 stats/runtime | 部分符合 | 缺少首屏 `config` 权威快照，初始化状态分散 |
| IPC Query 请求使用 externally-tagged：`{"request":{"runtime":{}}}` | 请求构造正确 | 符合 | 请求侧可与内核契约对齐 |
| IPC Query 响应 result 也 externally-tagged：`result.runtime`、`result.health` 等 | `unwrap_core_envelope` 只剥离 `ok/result`，没有按 QueryResponse 变体再解一层 | 不符合 | health/capabilities/flow/tun_status 等解析会丢字段或失败 |
| HTTP `result` 直接是内部数据，IPC `result` 多一层变体 key | 当前适配层主要面向 IPC，没有显式区分 HTTP/IPC result shape | 部分符合 | 后续接入 HTTP/SSE 时需要独立解析路径 |
| Query：`capabilities`、`health`、`config`、`runtime`、`stats`、`active_flows`、`recent_flows`、`flow`、`policies`、`policy`、`sinks`、`tun_status` | 已有 capabilities/health/config/runtime/stats/policies/active_flows/flow/tun_status；缺 recent_flows、sinks；部分解析未适配变体 key | 部分符合 | 连接页、事件投递页、TUN 状态和能力页不完整 |
| Command：`policies.select`、`policies.probe`、`flows.close`、`config.validate`、`config.apply`、`mode.set`、`tun.start`、`tun.stop`、`diagnostics.*` | 已有 select/probe/close/validate/tun.start/tun.stop/probe_target；缺 config.apply、mode.set、dns_lookup、trace_route；`policies.probe` 主要保留在 raw IPC | 部分符合 | 配置热加载、模式热切换、DNS/路由调试能力不足 |
| 事件：订阅后先收到确认帧，再收到裸 `ApiEvent`；按 `ok` 判断帧类型 | 后端会读取确认帧并按 `ok` 校验，再转发后续事件 | 符合 | 基础事件流正确 |
| 事件类型：flow、policy、stats、warning、config、ipc 等；未知事件不应固化枚举 | 已映射主要 flow/policy/stats/config/warning；未知事件转 `core.unknownEvent` | 基本符合 | 对新增事件容错较好 |
| 断流后用 runtime/stats 重建状态，Zero 未启动时按 offline | 订阅成功后会补拉 stats/runtime；断开后前端只标记 disconnected/offline，没有统一自动重连策略 | 部分符合 | 内核重启后可能依赖其它流程重新订阅 |
| `capabilities.protocols` 是协议能力源，包括 TCP/UDP/MUX/limitations | `GuiZeroCapabilities` 只保留 api/schema/features/permissions/adapters/sinks | 不符合 | GUI 无法基于能力矩阵判断协议兼容性和限制 |
| 配置顶层含 `mode`、`api`、`runtime.dns`、`route.url_rewrite` 等 | GUI 代理模式写入 `route.mode`，导出时移除并转成 `route.final`；未直接使用顶层 `mode` | 不符合 | 与最新内核配置模型和 `mode.set` 热切换能力偏离 |
| 编辑配置建议草稿 -> `config.validate` -> `config.apply` -> 重新查询 config/runtime | 当前有 validate；业务模式切换走导出配置 + 可选重启，没有 config.apply | 部分符合 | 不能利用内核热加载能力 |
| HTTP `/api/v1/*` 和 SSE `/events/stream` 可作为备选通道 | 当前 Tauri 后端未实现 HTTP 控制面客户端 | 未实现 | 当 IPC 不可用时没有备选本地通道 |
| GUI 启动后默认保持内核常驻，退出 GUI 时关闭托管内核 | 当前仅 `autoStart=true` 时尝试启动；默认 `autoStart=false`；关闭窗口只是隐藏，托盘退出后依赖 Drop 清理子进程 | 部分符合 | 默认不保证内核可用，用户体验依赖手动连接 |
| 无配置文件时仍可启动内核并启用 9090 Web API 或给出明确缺配置状态 | `core_process::start` 当前强制要求 `config_path`，无 active config 时直接报错 | 不符合 | 首次启动或无配置状态下无法常驻内核，也无法通过控制面给出权威提示 |

## 已实现亮点

- IPC 请求帧构造符合文档的 externally-tagged Query 请求格式。
- 后端已经把 Zero 原始事件归一化为 GUI DTO，前端业务层不直接解析内核原始事件。
- 事件订阅正确处理确认帧和后续事件帧，心跳行由 transport 层跳过。
- 策略切换、目标探测、连接关闭、TUN 启停已经通过 Zero command 接入。
- GUI 层已有业务接口隔离：常规页面使用 `gui_*`，raw IPC 保留为专业诊断入口。

## 主要未实现或偏离

1. IPC QueryResponse 外层变体没有统一解包，导致多个解析器实际读不到最新文档规定的字段。
2. Unix 默认 socket 路径仍是旧 GUI 托管约定，不是最新 Zero daemon 默认 `~/.zero/control.sock`。
3. 能力发现没有保留协议矩阵，能力页不能表达 `partial`、`experimental`、UDP/MUX 限制。
4. 配置模式仍是 GUI 私有 `route.mode`，没有使用最新顶层 `mode` 或 command `mode.set`。
5. 缺少 `config.apply`，配置变更无法走文档推荐的热加载闭环。
6. 缺少 `diagnostics.dns_lookup`、`diagnostics.trace_route`、`sinks`、`recent_flows` 等 GUI 高价值能力。
7. 项目自身 `docs/gui/core.md` 与最新内核文档在 Unix socket 路径和模式模型上已经不一致。
8. 默认启动策略仍是可选 `autoStart`，不符合“GUI 运行期间内核常驻”的新原则。
9. 无配置文件时无法启动内核，尚未支持“内核可运行但提示缺配置”的体验。

## 建议落地顺序

1. 修复 IPC QueryResponse 解包，这是后续所有 Query 能力可靠性的前置条件。
2. 调整或显式区分“连接外部默认 Zero daemon”和“GUI 托管 Zero 进程”的 socket 路径策略。
3. 扩展 `GuiZeroCapabilities`，保留 `protocols`、`build_features` 和 limitation codes。
4. 增加 `config.apply`、`mode.set`、`recent_flows`、`sinks`、DNS/路由诊断命令。
5. 将代理模式从 `route.mode` 迁移到顶层 `mode` 或运行时 `mode.set`。
6. 更新 `docs/gui/*`，避免 GUI 文档继续描述旧契约。
7. 调整启动流程：GUI setup 阶段默认检查并启动托管内核，失败时进入“缺配置/待配置”状态而不是静默不启动。
8. 与内核约定无配置启动方式：优先使用内核原生“无配置提示”能力；如果内核暂不支持，则由 GUI 生成最小临时配置并开启 `api.control.listen=127.0.0.1:9090`。
