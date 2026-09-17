# 统一客户端能力管道：入口盘点与迁移进度

2026-09-12。本地 P0/P1 及后续 P2/P3 实施记录，配合 [主方案](./unified-capability-plan.md)。已迁移订阅/规则网络读取、普通配置导入与运行应用，并移除托管启动的完整配置文件依赖；其余条目是明确的迁移债务。不是整个客户端已经统一；桌面 GUI 已可装载经登记/签名核验的受限组件，已开放自身信息与按 origin 授权的网络请求；配置投递和受保护订阅仍未接通。

## 入口、执行点与状态所有者

以下路径相对 `src-tauri/src/`；前端和 crate 路径另标。已扫描 commands、services、configuration、runtime_host、kernel 以及前端直接平台调用，按执行责任归并。测试中的模拟 IO 不作为生产入口。

| 入口/触发 | 实际执行与状态所有者 | 本批状态 / 下一步 |
| --- | --- | --- |
| `commands/subscription.rs` 手动同步、`services/subscription.rs` 自动同步 | AppState 管理同步去重和订阅状态；`fetch_subscription_content_blocking` → AppState capabilities → `client-capabilities::network::get` | **已迁移网络读取**；规则下载及最终配置应用已接共同执行器；订阅解析与本地持久化仍由原有业务链管理 |
| 插件 VM `runtime::execute` / 实验 `execute_network_lab` | `plugin-sandbox::policy` 只适配组件准入，执行租约委托 `client-core::capability`；`bridge` 委托共同网络执行器 | **已迁移正式网络桥**；桌面 GUI 的 execute_for_host 消费共同 Manager 授权，支持 network.get/network.request；独立 execute 默认仍无网络 |
| 插件旧 self/records 示例 | bridge 仅做可信身份和宿主预先选定统计的投影；共同 Lease 管理执行、结果资源和预算 | 保留示例，移除 VM 回调中的能力分派与插件私有租约实现；没有增加连接列表读取权限 |
| 规则手动/定时刷新、订阅附带 rule providers | `services/rule_set.rs` → 共同网络 GET（条件请求）；编译并写 zrs，`rule_overlay.rs` 读取编译物 | **网络已迁移**；保留 ETag/Last-Modified、304 和内容摘要，64 MiB 上限；编译物存储仍待迁移 |
| 内核版本目录、下载/升级 | `services/kernel_manager.rs` HTTP、校验、文件；`services/download/{transfer,cache}.rs` 管下载状态 | **未迁移**；保留大文件流式、缓存与原子安装语义，不能套 8 MiB 内存读取 |
| 客户端更新 | `commands/app_update/download.rs`、Tauri updater；有显式代理配置与包校验 | **未迁移**；代理和安装权限单独建模；不把更新能力给普通插件 |
| 系统网络检测 | `services/network_probe.rs` HTTP 与平台网络监听 | **未迁移**；网络读取接统一管理，平台事件订阅由适配器所有 |
| 配置文件导入、编辑、订阅内容投递 | 前端 ProfilesTab 选文件 → `services/proxy_config.rs` 读路径/正文、解析；`domain_store` / `app_database` 保存 | **普通导入已迁移**；宿主选定文件经 file.read，粘贴和文件经 configuration.prepare，16 MiB 上限；普通 SQLite 持久化、订阅解析及受保护材料仍待迁移 |
| 本地设置、DNS、模式、配置工作区应用 | `commands/{app_config,profile_settings,config_workspace,gui_core,...}` → `configuration` → `kernel/configuration.rs` / `kernel/zero` | **配置应用已迁移**；共同 configuration.apply → BoundControl → config.apply_runtime；DNS 与原始 IPC 的 apply 也转入此入口；其他模式/TUN/工具命令仍待迁移 |
| 启动/重启/恢复、TUN 和系统代理 | `runtime_host` / `services/core_config::export_active` / 平台 helper | **普通配置启动已迁移**；管理 bootstrap 启动后合成活动配置并经共同 runtime-only 应用，确认后才标记 Running；watchdog 复用此路径；受保护恢复与系统副作用授权仍待完成 |
| 节点测速、DNS、路由工具 | `services/{probe,tool_jobs}` 自有队列与持久恢复信息，底层 kernel 命令 | **未迁移**；已有 Job ID、取消/恢复、诊断继续由现有业务运行时所有，接共同能力上下文；不新增平行工具队列 |
| 连接观测与近期记录 | `services/flow_observation` → `kernel/observation` / engine-client；`connection_history_store`、`debug_store` 有自己的有界文件记录 | **未迁移**；只读快照、记录查询/导出均应按资源投影；不重做已有观测 |
| 调试、日志、诊断和内核设置导出 | `commands/gui_core.rs`、`services/{debug_store,log_store,kernel_settings}` 直接读写文件 | **未迁移**；文件授权与敏感内容出口治理必须覆盖这些路径 |
| 外部浏览器、目录、剪贴板 | Svelte 的 RulesTab/CoreConfigPanel/AboutPanel/AppConfigPanel 直接调用 opener；`src/lib/services/clipboard.ts` 直接平台写入 | **未迁移**；宿主页面的现有平台权限不等于未来插件权限；应迁入受控打开/复制能力 |
| 插件目录与安装 CLI | `plugin-sandbox/distribution/{remote,store}.rs` 自有 HTTP 与原子包存储 | **未迁移**；这是宿主安装管理工具，不是 guest IO；以后进入客户端安装管理能力，不能让 VM 获得其 reqwest/文件对象 |
| OAuth、秘密存储、通知的插件 SDK | 通用业务 SDK 尚不完整；桌面 GUI 已有受限组件的逐项授权入口 | **尚未实现**；不得以旧 RFC 的业务能力名称代表现有代码 |

已迁移的业务步骤仍分别建立内置调用租约；完整订阅任务的父调用关联、持久恢复和存储授权没有因此自动完成。第三方代码没有取得这些内置授权入口。

## 首批实际公共边界

- `crates/client-core/src/capability*`：runtime-neutral Manager/Policy/Lease/Resource。身份与许可由 native 宿主建立，来宾 JSON 不接受调用身份字段。Authority 是组件准入适配，不再持有另一套 enabled/epoch/busy/grants 状态机。
- `AppState::capabilities()`：GUI 的管理实例。订阅不创建自己的静态后台。GUI 插件所有者向 `Authority::admit_with_manager` 注入同一个实例；组件适配器现经 `Manager::admit_component`，重复准入共享授权和运行占用。升级/卸载退休旧句柄，旧调用退出前拒绝替换，替换后默认无授权。`Authority::admit` 仅为独立实验 CLI 的便利方法；GUI 装载、安装和卸载事务已接入；跨客户端进程授权同步仍待完成。
- `crates/client-capabilities/src/network.rs`：订阅和实验插件共用的 HTTP GET 执行器。reqwest 不进入运行中立 core，Tauri/VM/业务账号逻辑不进入执行器。
- 每次宿主调用建立一个租约，内部能力调用沿用此租约。共同执行处理运行前检查、调用额度、操作编号/父调用编号、完成后代次检查、资源预算。并发上限 32；保留最近 256 条完成/运行记录，满额不驱逐运行操作；记录不存 URL、请求头、响应正文。
- Resource 是不可序列化的宿主 Rust 对象，绑定确切租约，不能以相同字符串身份或另一个句柄 ID 读取。过期/撤权无法取出，drop 释放预留额度。**它还不是秘密内存容器**：没有清零保证、持久资源索引或重启恢复；网络实验明确返回可读正文，不承诺受保护数据。
- 一次执行因撤权/取消失去结果交付权，记录 `DeliveryDenied`，不宣称撤回已经发送的 HTTP 请求。阻塞中的网络读取可能等到返回或超时才结束；没有新增内核取消接口。
- 现有 probe/tool Job ID 不被这里的短时调用编号替代。共同业务父任务、用户可见操作页、持久恢复/重放和停用全局回收仍待迁入，P1 的 parent 只关联同一租约下的能力调用。

## 首批网络契约与行为变化

仅 HTTP(S) GET。目标用规范 origin 校验，包含 scheme/host/port；每次重定向发送前重查授权，最多 10 跳，拒绝 HTTPS 降级。拒绝其他协议、URL userinfo 隐含用户名/密码、超长 URL；错误不返回带 token 的 URL 或响应正文。

内置订阅沿用既有用户配置的 URL、User-Agent、HTTP(S) 重定向和 reqwest 默认代理策略。其 native 授权显式允许 Web origin，不把这个范围开放给插件。新的边界是最多 8 MiB 响应、30 秒整体租约、禁止降级与 URL userinfo；超限明确失败，不截断后当成有效订阅。query 参数仍可携带现有订阅 token。`subscription-userinfo` 由执行器保留，解释仍在订阅业务中。

插件网络仅接受 manifest 声明、注册上限和用户授权交集里的**确切 origin**（协议、主机、端口，不接受 `*`）。正式桌面入口现已开放 network.get/network.request，具体契约见文末及 [插件网络调用](./plugin-network-api.md)。不给系统 Cookie jar、浏览器会话或宿主凭据。当前没有额外的 IP 网段/DNS 重绑定过滤，不能将 origin 授权描述为网段隔离。

## 配置与存储进度（P3 部分完成）

- GUI `kernel/configuration.rs` 已统一调用 `config.apply_runtime`，`configuration/apply.rs` 经 engine-client 确认 instance/revision，同时要求回执 persistence=runtime_only。缺回执、失联或身份冲突不触发文件回退或自动重放。每个内核实例只允许一个受管配置提交；该约束不声称能锁住外部控制器。
- 只读核对本地 Zero：`crates/api/src/command.rs` 有 `ConfigApplyRuntime`；`crates/proxy/src/runtime/handle/command/dispatch.rs::execute_acknowledged` 把两种 apply 交给同一协调器，以 persist 标志区分；同步 execute 明确拒绝二者。engine 的 runtime apply 不保存配置。**已验证目标二进制 v0.0.2-dev.202609110445（de97b05）**：私有子进程接收 runtime-only 配置，新增 mixed 监听器完成 SOCKS 握手，实例未变、revision 递增、启动文件字节不变。此结果不代表其他版本兼容；无静默文件回退。
- `services/app_database.rs` 本节盘点时为 schema 1（后续普通能力存储已升级至 schema 2，见文末），profile 的 content 位于 `payload_json`，订阅另存 source_url；`configuration/persistence` / domain_store 负责发布。旧 SQLite/WAL、导出、备份、调试 preview 都在受保护迁移范围内；本批不改 schema、不删原文件。
- `configuration/local_edits.rs` 实际以 profile ID 保存 `profile_edits`，localProxy.host/port、bypass、DNS、urltest 等会改有效来源副本；`view` 还能投影 sourceEndpoint/sourceBypass。不能把这些字段直接用于受保护 view。
- 本地覆盖迁移表：端口/监听范围/设备旁路目标归属设备；已有 profile 专属编辑保留为资源级显式覆盖；策略选择按现有归属核对为资源/实例状态；DNS、测速、规则保留原有偏好含义。当前 profile 下的端口值不得直接批量提升全局，出现冲突需有明确兼容迁移规则，不能来源 A 切换后覆盖 B。

## readiness 失败归因

保留 1 秒测试预算与“拒绝其他 PID、关闭连接”断言。原数据目录下复现 `core readiness probe timed out`，耗时在 connect 返回前；临时计时确认首条 subscribe 调试记录同步 append/轮转曾用约 840ms（其他复现已耗尽 1 秒），不是内核返回错误 PID 后的判断失败。相同测试二进制使用全新 `ZNET_SINK_DATA_DIR`，首条记录约 1.5ms，整个测试 0.01s 通过。计时代码已移除，保留更清楚的错误断言。

这说明 IPC 握手关键路径仍受同步诊断文件 IO 影响；不是本批网络重构造成。后续日志/观测能力迁移需把诊断写盘移出 IPC 关键路径；本批不修改生产日志结构、不延长测试超时。最终回归使用独立临时数据目录，避免本机历史日志影响验收并避免写入用户真实数据。

## 验收方法

- `client-core/tests/capability.rs`：跨租约读取、权限范围、撤权/取消晚到结果、资源释放与调用额度、操作历史容量、并发上限。
- `client-capabilities/tests/network.rs`：真实 HTTP 请求在途撤权、协议/凭据 URL 拒绝。
- `plugin-sandbox/tests/network.rs`（显式 feature）：真实 VM 与内置 subscription lease 共享 Manager 和执行器；授权外重定向实际未到达第二个监听器；伪造内置身份和默认 VM 网络入口被拒；未知长度响应超限。
- GUI `services/subscription_capability_tests.rs`：实际订阅下载函数与 AppState 接线、userinfo 保留、管理记录无 token。
- `scripts/test-client-capability-boundaries.mjs`：禁止已迁移订阅恢复独立 HTTP、禁止 VM/准入适配层访问原生 IO/内核，限制 crate 依赖方向。该检查只约束已迁移路径，不代表所有历史旁路已关闭。

下一项应完成受保护材料与存储策略，复用当前配置应用/启动路径，并补齐用户授权和宿主组件生命周期。第三方网络/密码学/通知 SDK、所有敏感出口及多端验收仍按 P2–P5 推进。不能因普通配置使用内存应用就宣称受保护订阅已安全。

## P0/P1 验证记录（保留历史）

最终实现于 macOS x86_64 验证：

- 独立临时 `ZNET_SINK_DATA_DIR` 下，`cargo test --manifest-path src-tauri/Cargo.toml --workspace --all-features --locked`：**596 passed / 0 failed / 6 ignored**。日志 `/tmp/unified-final-workspace.log`。
- 6 个忽略项包括真实发布内核/观测契约、真实订阅访问和一个文档示例；不作为真实内核或线上业务验收通过。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --locked -p znet-client-core -p znet-client-capabilities -p znet-plugin-sandbox --all-targets --all-features -- -D warnings` 通过。仅顺手移除 client-core 既有文档注释后的一个空行以满足 lint，没有改该函数逻辑。日志 `/tmp/unified-clippy.log`。
- `node --test scripts/test-client-capability-boundaries.mjs`：3 passed；Rust 格式与 `git diff --check` 通过。
- 没有前端交互变更；没有进行插件 GUI 装载、实际发布内核应用、真实 ZBoard、Windows/Android/iOS 验收。没有修改 Zero 内核或中央目录，没有提交/推送。

## 本轮新增实现与验收

- 公共执行支持 async；future 被丢弃记录 DeliveryDenied(Cancelled)，在途撤权拒绝交付；目标占用守卫在 drop 时释放，不持锁跨 IO。同步/异步共用操作记录和预算。
- 普通文件导入使用不可序列化的宿主文件选择对象；非普通文件、超限与其他选择的授权不能通过；尚未向插件开放文件接口。
- 订阅/规则网络仍由统一执行器负责，规则新增条件请求，304 不是下载正文；普通导入与最终配置应用都产生共同管理记录。
- 连接、配置切换、DNS 应用不再隐式导出完整活动配置。托管启动与 watchdog 在管理 bootstrap 就绪后应用当前活动材料；显式普通导出仍保留，旧用户文件不删除。
- config 字段在 IPC 预览、debug 内存记录/磁盘记录之前统一遮盖；畸形 JSON 预览不记录原文。canary 与 Unicode 边界测试覆盖此入口。**这不是完整敏感出口治理**：普通 profile/SQLite、其他日志/查询/导出和内核外部控制面尚未实现保护政策。
- 真实内核测试 `supplied_zero_confirms_config_application_without_restarting` 通过；日志 `/tmp/capability-real-runtime.log`。测试只创建私有子进程与临时目录，没有修改或重启用户当前内核。
- Node 22 使用仓库 alias loader：能力架构边界、DNS、配置工作区/激活及 runtime lifecycle 共 17 项通过，日志 `/tmp/capability-js-check.log`。
- 全工作区 all-features 回归：603 passed / 0 failed / 6 ignored；日志 `/tmp/capability-followup-final.log`。忽略项中的真实配置应用另行显式执行通过；真实订阅、其他忽略项与跨端验收不计作通过。公共 core/capabilities/plugin 三个 crate 的 all-targets/all-features 严格 Clippy 通过，日志 `/tmp/capability-followup-clippy.log`。
- 本轮没有修改 Zero 内核、没有发布/推送；P2 尚有存储、工具/观测、更新和平台副作用旁路；P3 尚缺受保护材料及恢复；P4/P5 未完成。

## 组件生命周期后续实现（2026-09-12）

- `client-core::capability::Manager` 持有组件注册表，按宿主确定的 plugin/component 键及材料摘要绑定授权；组件适配器不会因重复加载创建另一份 enabled/busy/grants。普通内置短操作仍使用既有 admit，不改变业务队列。
- 升级/卸载使旧 Policy 永久退休，旧 Authority 不能重新授权；旧调用尚未退出时保留占用，拒绝启动替代版本。下一次准入回收已结束的退休记录；最多 64 个注册项。
- 统一快照提供声明、必需/已授权限、启用、运行、授权版本和注册代次。授权决定核对材料身份、注册代次和授权版本，防止停用、升级或同版本卸载重装后旧授权对话框重新放行。授权暂不持久化，客户端重启默认没有组件授权。
- 正常退出与生命周期兜底均调用 shutdown_components，撤销现有组件并拒绝新准入。VM 已消费同一租约的中断检查；guest 异常释放运行占用，保留仍有效的用户授权。阻塞宿主 IO 仍遵守现有超时，不能把撤权称为撤回已经发送的请求。
- 后端入口与 VM 实验调用已接通；**GUI 插件加载、授权界面、安装/卸载事务调用这些入口，以及多进程状态同步仍未实现**。本批不开放第三方网络、文件或配置权限。

尚未落地的下一段依赖：受保护材料/秘密存储及重启恢复；权限展示与用户决定闭环；通用网络请求、密码学、通知等 SDK；所有日志、查询、数据库、导出和内核控制面的保护策略；最后才是插件自行实现 ZBoard 业务及多端实测。普通 runtime-only 应用不等于这些保护已经完成。

本次组件生命周期定向验证：client-core 与 plugin-sandbox 的 all-features 测试 **65 passed / 0 failed**（`/tmp/capability-component-tests.log`）；架构边界 **6 passed**；core/capabilities/plugin 三个 crate 的严格 Clippy 通过（`/tmp/capability-component-clippy.log`）。没有重跑未变化的真实内核验收，也没有宣称 GUI 授权交互已经验收。

## 桌面 GUI 装载与用户授权（2026-09-12）

2026-09-14：插件已移为独立顶层菜单，新增“发现插件 → 选择发布版本 → 下载并安装”；原本地安装保留为辅助入口。

前一轮完成的插件管理：检查已安装插件、导入签名包、查看权限、明确授权、运行、停用、卸载。正式入口不接受自定义目录地址、公钥或 JS 正文。读取固定中央目录复用 client-capabilities 网络执行器，文件选择复用 file.read，安装/核验使用共同管理记录；VM 始终使用 AppState 的 Manager。

- 安装目录为客户端数据目录下 `plugins/`，复用既有签名 Store；CLI 指向同一 root 时可共用安装数据。包由发布者分发，客户端校验中央登记和发布者签名。GUI 不实现市场发布，不修改中央登记。GUI 已接入 Release 列表与在线安装；尚无自动升级或回滚按钮。
- 中央目录有效期最多 10 分钟，每次授权/运行重读并验证本地安装包；目录过期要求重新检查。没有离线授权恢复、没有静默信任旧目录。缺登记、签名篡改、校验失败或卸载会使已有组件失效。
- 安装不授权，授权必须包含用户勾选的必需权限；用户可取消。许可最多持续到本次目录校验到期，客户端重启后重新确认。授权窗口绑定材料、注册代次和授权版本，不能跨升级、卸载重装或撤权生效。
- **本节初次交付仅开放 plugin.self.read / self，后续网络扩展见文末**。当时网络、文件、连接摘要、配置与内核权限均不能通过 GUI 授予；必需能力未开放的组件显示不可用，可选未开放权限显示但不可选择。guest 不获得原始 Tauri、文件路径、Cookie、配置正文或内核句柄。
- 正常运行、错误与停用均回到同一组件状态；停用不等待安装网络 IO，晚到结果不会在页面显示成成功。卸载有用户确认，作用于插件的全部组件。外部 CLI 改动在下次检查/授权/运行时检测，不宣称存在跨进程推送撤权。
- Android/iOS 不引入此桌面运行依赖，界面明确当前端尚不支持；macOS 本地编译和浏览器交互已验证，不代表其他端实际验收。

验收：全 workspace / all-features Rust **617 passed / 0 failed / 6 ignored**（`/tmp/plugin-gui-tests.log`）；Chrome 页面两项测试覆盖必需权限、不可选权限、旧授权窗口、停用晚到结果与卸载确认（`/tmp/plugin-gui-ui-tests.log`），浏览器使用模拟 Tauri 返回，后端另以真实签名包和真实 VM 执行。Svelte 检查 0 错误/警告。中央目录在线核对仍为空，本轮没有登记或安装生产插件。

剩余主线仍是受保护材料、密文存储、秘密句柄、重启恢复、全部敏感出口治理和通用网络/密码学 SDK，最终由插件自行实现 ZBoard 业务。不将本次权限界面等同于受保护订阅已完成。

本轮最终补充：`pnpm build` 通过（`/tmp/plugin-gui-build.log`），能力架构边界 7 项通过，全 workspace/all-targets/all-features Clippy 完成（保留既有非插件代码 warnings，`/tmp/plugin-gui-clippy.log`），格式及 diff 检查通过。未修改 Zero 内核，未提交、推送或发布。


## 后续：网络权限接入正式桌面入口（2026-09-12）

网络、配置及受保护订阅均属于本轮体系必须支持的范围，不把暂未开放作为最终能力清单。分批交付不改变这一目标。

- 桌面权限清单可选择 `network.get` 与 `network.request`，都需中央登记的能力上限、签名 manifest 声明和用户明确授予。签名或安装本身不授予网络。
- `network.request` 支持 GET/HEAD/POST/PUT/PATCH/DELETE、受限请求头、UTF-8 或 Base64 请求体，返回状态、content-type/location 和正文。插件可自行实现账号协议，客户端不增加 ZBoard 登录业务。
- 通用请求不自动跟随重定向；插件需对下一 URL 再请求授权范围内的网络调用，不自动转发凭据或请求体。`network.get` 保留逐跳检查，跨 origin 清除条件请求头。
- 禁止 Host、连接/代理/传输控制请求头；拒绝 URL 用户信息。响应、时间、调用次数受原租约控制，网络记录不含请求 URL、请求体或 Authorization 值。撤销会禁止后续调用及晚到结果，不能撤回已发出的 HTTP 副作用。
- 当前 VM 单次执行仍至多 2 秒、输出及资源预算 64 KiB，网络响应 JSON 编码也占输出预算。这是小请求能力，尚不能据此宣称大订阅下载与登录工作流可用。没有系统 Cookie jar、后台调度、原生账号/秘密访问；也没有 IP 网段/DNS 重绑定隔离策略。

下一批配置接入必须把原调用租约贯穿配置准备、保存和实际提交，不能入口检查后重新获得内置授权；必须复用现有 profile 管理、合成、runtime-only 应用及启动恢复。受保护材料仍缺密文存储、密钥适配、正文出口限制和恢复实现，不能写入普通 content/SQLite 后再隐藏 UI，也不能另做插件临时应用通道。

实现与验证入口：`client-capabilities/tests/request.rs` 校验真实 POST、二进制体、禁止凭据自动重定向、GET 权限不能升级和 Host 伪造拒绝；`plugin-sandbox/tests/http_host.rs` 校验正式 VM 网络桥；`services/plugins/tests.rs` 用签名包、真实 Host 审批及 VM 验证授权前/停用后均不产生请求。


本批验证：前端检查 0 错误/0 警告，Chrome 权限/停用测试 2 项通过；串行工作区运行中 GUI 438 项通过（4 项忽略），新增真实网络 Host 测试通过。该次工作区因旧沙箱请求大小断言中止；更新为 128 KiB 边界后，plugin-sandbox 全部测试重跑通过。最初并行回归还暴露既有 readiness 用例失败及新增测试 socket 非阻塞夹具问题，后者已修正。这里不将首次失败的工作区命令记作全绿，也不将网络测试当成受保护订阅验收。未提交、推送或改动 Zero 内核。


## 客户端自身存储与定时细化

新增 [客户端存储、执行与定时边界](./client-state-boundaries.md)，逐项区分实际入口、此次代码与剩余工作。普通隔离存储复用现有 SQLite，订阅/规则定时刷新已作为真实内置消费者接入重试快照保存与恢复；插件业务和插件定时 API 不在本批实现。

## 客户端内部入口收敛（2026-09-17）

- 内核、应用更新、插件 Release 安装和内置规则更新复用 `client-capabilities` 网络执行器；大文件下载保留缓存、Range/If-Range、强 ETag、有限重试、大小上限和进度回调。每次重定向重新检查许可，HTTPS 不允许降级；插件 Release 只允许 GitHub 及其发布资产域名。
- 规则 ZRS 和内核设置导出通过宿主选定目标的 `file.publish` 原子发布；用户导入通过不可序列化的 `file.read` 选择对象。内置规则的启动期内嵌资源安装仍属于 bootstrap，但线上更新写入已进入同一发布能力。
- 系统网络探测进入统一网络能力；TUN、系统代理、代理模式和诊断工具作为客户端原生操作记录。打开 URL/目录、在文件管理器定位及剪贴板写入统一通过宿主命令校验和记录，UI 不再直接导入 Tauri opener。
- 诊断任务在能力 SQLite 中保存经过遮盖的有限元数据。启动记录保存成功后才执行；正常退出先中断并保存活动任务，异常退出后的下次启动把遗留活动任务标为 `client_restarted`，不会自动重放。能力操作 scope 包含任务 ID，可关联具体执行。
- 持久日志在写入内存和文件前统一遮盖敏感键及 URL；诊断导出、历史导出、日志/调试清理通过原生操作入口。Manager 全局关闭后会撤销现有 native/guest 租约并拒绝新准入。
- 这些变更完成客户端自身旧路径的边界收敛。2026-09-17 进一步完成通用插件 SDK v1：通知、持久调度意图、一次性浏览器回调、用户文件选择、短期秘密/材料句柄、受限密码学和受保护 runtime-only 应用均进入同一能力与租约检查；Rust/TypeScript 契约见 [插件 SDK v1](./plugin-sdk.md)。Connect 登录、远端设备凭证续期和订阅关联属于插件及 ZBoard 服务端，不进入客户端业务实现。
