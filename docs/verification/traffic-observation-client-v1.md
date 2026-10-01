# Zero Traffic Observation V1 客户端对接验收

日期：2026-10-02。仓库：`/Volumes/tool/rust/gui`。仅修改客户端，未构建、修改、替换或重启内核，未对真实内核执行清空。原工作区另有节点、端点控制和 DNS 在途修改，保留这些修改；下列范围仅为本次统计对接。

## 契约与运行能力

已读取对接提示词、内核 `docs/project/traffic-observation-v1.md`，以及 `zero-api` 的 `traffic.rs`、`query.rs`、`command.rs`、`capabilities.rs`、`event.rs`。按源码的类型与字段实现，无 WireGuard 专用统计命令。

对现有客户端所用内核进行只读 IPC capabilities 查询：V1 contracts 可用，但没有三个 `traffic_*_v1` 功能声明，`traffic_statistics` 为 null。其构建包含 Connector，这不构成新统计能力的证据。因此本机现有安装只能验收旧内核能力判定，不能证明新内核的真实流量计量、采样或清空成功。

实际对接传输是现有本地 IPC（Unix socket / Windows named pipe）。查询读取 `ApiResponse.result.traffic_stats` / `traffic_stat`，不会把 HTTP 直接结果快照混入 IPC。命令使用现有 `stats.reset` 帧。无新增 HTTP、SSE、FFI 或 Connector 客户端通道。

IPC 现有订阅正常时复用全局 `gui:event` / `gui:event-status`，页面退出仅清理自己的监听器。只有初始 subscribe 明确返回 unsupported / permission_denied、能确认业务请求尚未发送时，才使用同端点的已有单次 IPC 请求传输。超时、关闭及业务命令失败不会触发重发。无可用事件订阅时使用批量分页轮询。

当前 Zero 本地 IPC 根据 OS socket/pipe 访问授予 ipc-local Read/Control/Config/Admin；capabilities.permissions 是适配器元数据，不是调用者授权回执。客户端按此现有传输契约开放 Admin 动作，内核仍校验命令权限，permission_denied 后禁用清空。没有绕过鉴权。

## 已实现

- “端点”工作区内增加“流量统计”视图，支持 global、inbound、outbound、endpoint、peer；五种身份独立，不从旧端点目录猜测新资源身份。资源/peer ID 从查询库存读取，作为不透明 ID。
- 查询优先分页批量，默认每页 64（尊重内核更小的最大页大小）。后续页带首页实际实例、config_revision、registry_revision；conflict 最多重开整轮三次。只有完整成功轮次删除库存身份。客户端库存上限 16,384，超限显式失败，保留原库存。
- Flow、Inner、Outer 分开，按执行角色选择业务上下行或 RX/TX；显示实际 source_roles、accounting_basis、指标可用性。没有平面/设备/逐跳求和，不使用完成连接事件或 legacy per_outbound 重建累计统计。
- 计数、revision、generation、单调/墙钟时间经原生边界转十进制字符串，使用 BigInt 差分。新增事件有无损 sequenceExact；旧事件字段保持兼容。null 或不在 available_metrics 的值显示“—”，真实零显示 0。未知新增指标在详情中按实际声明展示。
- 同实例、范围、epoch、generation 内按实际单调时间差计算 bytes/s。第一样本、重启、周期变化、计数下降、不可用指标或非递增时间不产生伪速率。曲线墙钟只负责定位。
- 历史限制两分钟、每范围 121 点、最多 64 个范围保留曲线；统计视图每屏最多 24 个卡片。端点卡片复用同一库存及历史，无逐端点统计轮询；详情独立展示三平面。
- 订阅 stats.scopes_sampled、stats.reset 经已有 GUI 事件归一化。序列检查覆盖整个共享事件流，包含非统计事件，避免把正常事件过滤误判成缺口。分页采样只更新所含范围。
- 断线、序列缺口、原生广播 lag 的 subscribed 重同步、新 generation/未知 epoch/实例都会查询恢复；旧实例、已结束周期和非递增样本不能回滚。同步未完成时缓冲事件，不让迟到样本建立恢复基线。无订阅每 3 秒查询；实时模式每 30 秒库存核对，失样阈值按轮转页数放大，查询失败至少隔 3 秒再轮询。
- 初次能力查询失败每 3 秒恢复检查；明确不支持的旧内核每 30 秒重新检查能力，不发送未声明的统计查询。已缓存能力在内核降级后收到 unsupported 时重新发现能力；即使内核声明与查询实现不一致，也不会产生即时重试循环。
- 管理员明确选择 1..min(256, 内核上限) 个 scope，确认列出影响范围。计划锁定实际实例、epoch，以及非 null 的 generation；无 generation 时省略。命令不含 metrics 子集、不做 global 级联、不操作设备/连接/配置。
- 使用匹配确认的 snapshots 建立新周期、时间和累计值基线，允许并发 I/O 后的非零累计值，活动状态不改；仅清理确认影响范围的曲线。
- 重复点击提交一次。丢失/不匹配确认后只查询恢复，不生成新周期重置计划，不自动重试。显式处理 permission_denied、conflict、not_found、invalid_argument、unsupported。删除范围不会留下无法清除的界面选择。

## 本次文件

### 新增

- `src/lib/features/traffic/{types,wire,policy,inventory,history,stream,client,session}.ts`
- `src/lib/components/tabs/{TrafficStatisticsPanel,TrafficScopeDetails,TrafficMiniChart}.svelte`
- `src-tauri/src/commands/traffic_observation.rs`
- `src-tauri/src/services/traffic_observation.rs` 与 `traffic_observation/{types,tests,ipc_tests}.rs`
- `src-tauri/src/kernel/zero/traffic.rs` 与 `traffic/{wire,tests}.rs`
- `scripts/test-traffic-observation.mjs`
- `tests/fixtures/traffic.ts`
- `tests/ui-controls/traffic-observation.ts`、`traffic-observation.spec.ts`
- 本文档。

### 扩展/修改现有文件（包括原先尚未提交的端点组件）

- `src/lib/components/tabs/{EndpointsTab,NetworkEndpointsPanel,EndpointCard}.svelte`：工作区入口、共享统计投影、仓库统一表单控件。
- `src/lib/types/{gui-api,core}.ts`：能力、归一化事件、无损序列类型。
- `src-tauri/src/kernel/{protocol.rs,zero/mod.rs,zero/parsing.rs,zero/events.rs}`：现有传输复用、能力/事件边界。
- `src-tauri/src/models/gui_core.rs`、`src-tauri/src/services/gui_events.rs`：能力及事件模型。
- `src-tauri/src/{application/commands.rs,commands/mod.rs,services/mod.rs}`：命令注册。
- `package.json`：统计测试命令及前端验收链；`tests/ui-controls/vite.config.ts`：仅测试夹具别名。

## 验证结果

- `pnpm verify:frontend`：通过，包括统一 UI 控件契约、其余现有行为套件、Svelte 检查、生产前端构建。最终统计套件单独复测 21/21；最终 `pnpm check` 为 0 errors / 0 warnings，`pnpm build` 成功。最终补充用例覆盖初次能力查询失败、运行中内核升级/降级及声明与查询不一致时的有界恢复。
- Playwright（本机 Chrome）：统计交互 6/6；旧端点交互回归 16/16。统计详情 752px / 360px 无横向溢出。测试夹具明确隔离真实内核，模拟数据没有进入产品路径。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`、workspace locked all-targets check、`git diff --check`：通过。
- 原生统计新增 10 项测试通过，包括孤立 Unix IPC 的正确判别包装、u64 最大值、确切 CAS、拒绝权限、业务错误、确认丢失不重发，以及无订阅的单次请求退化。
- 默认环境 Rust workspace 全量：587 passed / 1 failed / 5 ignored；失败为已有 `services::network_probe::probe_tests::all_failed_probes_identify_the_check_without_disclosing_configured_urls`，其断言要求“建立连接失败”，实际受继承代理环境影响。仅在测试进程为 localhost/127.0.0.1 配置 NO_PROXY 后该单测通过，未修改产品网络探测实现。
- `NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked -j 1`：全量通过，41 个 harness，812 passed / 0 failed / 7 ignored。忽略项不算实机验收；保留已有编译警告。

## 尚未覆盖

- 本机安装内核未发布 Traffic Observation V1，未进行真实 TCP/UDP/原生 Packet/WireGuard 计数、长期大库存掉样及真实并发收发中清空验收。相关客户端状态转换和边界使用隔离 IPC、逻辑及浏览器夹具验证。
- 未在 Windows named pipe、Linux 桌面或系统 WebView 真机运行本次功能；跨平台复用既有 IPC 传输，不能把 macOS 测试写成其他平台已验收。
- HTTP/SSE、FFI 和 Connector 并非本次客户端实际传输；无新增这些通道或回放实现。IPC 缺口使用权威分页查询恢复。
- 不提供内核尚无真实来源的指标；未知/不可用值保持“—”。未修改内核，无发布、提交、推送或安装操作。
