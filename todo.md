# 移动适配依赖与暂不支持项

2026-10-18 增补：可安装 Debug 宿主与 USB 调试见 [指南](docs/gui/android-debug-usb-v0.0.3.md)。
它使用临时开发签名和独立 `.debug` 包，不解除下列内核/VPN/生命周期条件；Android 原始许可
材料仍有四项缺口。最新 develop `408327df53554c28d5efeda3ece65535cabf783c` 与本工作分支存在
真实冲突，本批未合并；新增分支 push CI 只验证精确 head，不能用作合并后或设备验收证据。

2026-10-14，v0.0.3，Draft PR #46。这里是依赖协调与解除条件的单一入口；完整 PRD 范围和已实现项沿用 [验收账本](docs/gui/mobile-acceptance-ledger-v0.0.3.md)，不另造完成标准。源码检查不是手机验收，unsupported 占位不计为已完成 VPN。用户已确认内核接入、原生VPN及宿主生命周期装配后置；该优先级不影响其验收条件，当前以独立客户端功能交付。

已核实客户端 develop 为 `fd198ba52711f018334860b8a6704546ccd349fd`；客户端依赖和只读 core 工作树固定 `e74561f99acca941bfadf0d33e35a5ef08f34086`。下列 core 源码证据均针对该固定依赖。`a51c6ce` 的实际桌面 DNS 契约 CI 另验证了 develop `0926da3ea4f51a2b508a0cf31074fc2a1046b8ef`；这不升级客户端依赖，也不证明移动接口或 non-desktop TUN 编译问题已解除。只修改客户端；core 由用户协调其负责人。下列行为要求是建议验收契约，接口名称尚未决定。

## CORE-01

**必需阻塞：外部平台隧道接入。依赖方：core 的 tun/proxy/stack 及客户端原生宿主。**

证据：[`TunDevice::into_channels[_observed]`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/tun/src/lib.rs) 已存在；`adopt` 只在 Linux/macOS 导出。Proxy 的 [`start_tun_internal`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/proxy/src/inbound/tun.rs) 仍自行 create/configure，并走桌面路由安装。

影响：Android VpnService 的 FD、iOS Packet Tunnel 的 packetFlow 尚不能正式注入完整 Proxy，不能开启或确认移动 VPN。

解除条件：支持平台已经配置的设备或包通道进入既有 stack/proxy；明确包方向、IPv4/IPv6、MTU、背压、错误、关闭所有权与真实设备统计。不重新创建桌面接口或重复配置系统 DNS/路由。

验收：真机 TCP/UDP/DNS/Fake-IP 和声明协议可双向转发；停止和撤权释放资源；背压、接口失败和服务重建可核对，不假报接管。

## CORE-02

**必需阻塞：移动外层 socket 保护与物理网络绑定。依赖方：core traits/platform/各实际拨号路径及客户端宿主。**

证据：[`EgressInterfaceControl`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/platform/tokio/src/egress.rs) 已有 generation、subscribe_generation、invalidate_network、双栈出口和桌面绑定；Android/iOS 对应绑定分支返回 Unsupported。[socket 构造路径](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/platform/tokio/src/lib.rs) 没有移动宿主保护回调。不能说内核完全没有网络代次机制。

影响：Android 需要连接/发送前 protect 与 Network 绑定；iOS 需 provider 允许的外网连接路径。未接入时不能保证外层连接不回送 VPN，不开放接管。

解除条件：所有真实 TCP/UDP/DNS/QUIC/协议外层 socket 在第一次网络 IO 前进入正确保护/绑定边界，失败拒绝拨号；平台网络身份及路径变化接入现有代次/失效机制，旧连接池与拨号结果不能继续代表新网络。

验收：Wi-Fi/蜂窝切换、双栈/IPv6-only、断网及恢复无回环；保护失败明确失败；旧网络 late result 不宣称恢复成功。物理出口缺失与禁用 IPv6 分开表达。

## CORE-03

**特定写操作安全缺口；不是所有操作的统一阻塞。依赖方：core API 与 owning command。**

证据：[通用 ConfigApplyCommand/ModeSetCommand](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/api/src/command.rs) 没有相应 expected instance/config revision。端点、统计已有自己的前置条件；[`ProxyHandle::apply_runtime_config_if_current_with_principal_impact_and_wait`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/proxy/src/runtime/handle/model.rs) 已在配置应用锁内比较当前配置。

影响：Android/iOS 服务重建或另一个控制端在读取和提交之间改变配置时，客户端本地 CAS、固定连接及前后查询不能冒充通用远端原子保证。

解除条件：对确有需要的公共写命令提供兼容的身份/版本前置条件，并在 owning command 同一协调边界校验。Rust 内嵌可先复用现有条件应用路径；不向已具备条件的端点/统计重新发明一套契约。

验收：两个控制端竞争、同名资源重建及实例重启使旧写操作在副作用前被拒绝；超时先核对、不自动重放。无安全条件的能力保留明确限制。

## CORE-04

**条件增强：统一外语 FFI 的完整生命周期；不是 Rust 内嵌的必需前置条件。依赖方：选定桥接方案后由 core/客户端协调。**

证据：[当前 C FFI](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/ffi/src/lib.rs) 创建 EngineHandle；公开 Rust [Proxy::new/spawn](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/proxy/src/runtime.rs)、[RunningProxy::shutdown](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/proxy/src/runtime/running.rs) 和 ProxyHandle 的查询/命令/事件服务已存在。

影响与解除条件：客户端 M01/M04 可独立基于公开 Rust 接口实现；仅当采用 C 边界时才扩展该包装，范围限完整运行所有权、停止和已有控制/事件服务，不把客户端安装、订阅或插件业务塞进 core。

验收：原生服务/扩展持有实际 Proxy，UI 退出不终止转发；停止完成资源释放；不能用 EngineHandle 的控制状态冒充数据面就绪。

## CORE-05

**实际移动目标编译阻塞：非桌面 TUN 工厂返回类型。依赖方：core 的 tun crate。**

证据：客户端真实 Android API24 NDK `cargo check --target aarch64-linux-android` 在固定 e74561f 的 [`crates/tun/src/lib.rs:165`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/tun/src/lib.rs#L165) 报 E0282。`create() -> io::Result<impl TunDevice>` 在非 Linux/macOS/Windows 只有 `Err`，没有可确定的 concrete TunDevice。没有修改 core 或缓存源码。

影响与解除条件：完整 Proxy 即使不启用 TUN 也依赖 zero-tun，因此移动目标编译被阻塞；需要上游让 unsupported-target 分支具有合法、明确的返回类型并保持 Unsupported，或通过正式外部设备接口/feature 组织补齐。不能用移动平台伪装为 Linux，不能替换成 EngineHandle 控制状态。

验收：固定修复版本的 Android API24 NDK 与实际 iOS SDK 完整 Proxy compile/check 通过，并覆盖明确拒绝未适配设备；这不替代 CORE-01/02 的真实接入及手机网络验收。iOS owner SDK 本轮未运行，因相同非桌面分支问题保留阻塞；仅已有 Swift 文档库及轻量共享 Rust 目标检查可按实际结果报告。

## CLIENT-01

**原生 runtime owner、默认随包内核与控制桥：客户端继续实施。** 本轮增加共享 Rust 窄控制桥、实际 IPC 只读检查和独立监听取消/缺口处理；native owner 与 EventSource 尚未注册，不计为完整 M04。 已有公开 Rust 生命周期/控制服务可复用；本轮新增 [具体 RuntimeOwner](docs/gui/embedded-mobile-runtime-v0.0.3.md)，独立 Tokio executor 实际持有 Proxy/RunningProxy，UI观察句柄销毁不停止转发，停止等待 orchestration 和 executor disposal。Linux真实回环/控制测试不等于 native host 注册。当前没有真实 VpnService/Packet Tunnel runtime 注册或可运行的默认随包内核，移动调用返回 unsupported，平台 VPN 状态保留 unknown。解除条件为实际宿主所有权、随包编译能力、鉴权/有界控制与事件桥及 CORE-01/02/05 组合验证；验收包括撤权、主动断开、实例换代和不依赖 UI 存活。

## CLIENT-02

**后台订阅和更新检查：客户端及平台调度，继续部分实施。** 已有真实前台到期协调；后台 OS 调度仍未适配，不注册空 Worker 或承诺锁屏准时执行。普通条件同步复用共享 HTTP executor，validator 绑定有效来源/材料，304 不重复 apply，暂停自动参与仍允许明确手动同步。

本批增加客户端可独立完成的“已持久 confirmed 应用、仅本地发布中断”恢复，限普通活动 Zero 配置的真实运行变更且不需外部 OS 系统代理 retarget：schema 3 有界候选/回执账本和五阶段屏障；主窗口只读核对原记录、版本及 pinned IPC 身份，再由用户明确完成原候选的 SQLite 关系/AppConfig 投影。不重新下载、提交 apply 或操作 TUN，不把 local-committed 当 complete，不覆盖随后改变的源、关系、设置或独立 typed 规则集（只保存摘要）；重复完成须当前事实与原完成摘要精确匹配。条件 validator 不属于本批恢复收据，未保存时明确未恢复，后续独立同步按正常绑定流程处理。实际测试及平台未运行项由[统一验证记录](docs/verification/mobile-client-v0.0.3.md)列出；详见[核对与恢复边界](docs/gui/ordinary-subscription-reconciliation-v0.0.3.md)及[存储限制](docs/gui/storage.md)。

仍未覆盖本地恢复：托管系统代理的实际 host/port/native bypass 需要 retarget 或新鲜状态无法确认时，继续原真实 retarget/补偿链，不静默跳过高级能力。

仍未完成：没有持久 confirmed 回执的历史/丢失回执操作、确认落盘前中断、实例/revision 或组合漂移不能解除；当前公开配置投影不足以证明原提交，需要可关联原操作的权威结果或允许读取的运行摘要，再与 CORE-03 owning API 协商。移动运行 owner 未注册时只读展示本地事实，恢复准入明确 unsupported；原生 VPN/core 装配、后台调度和真机恢复继续后置。不能清缓存、重复下载或仅看当前 revision 来绕过，也不能把全部订阅工作归为 core 阻塞。

## CLIENT-03

**活动连接导出、捕获恢复记录：本轮源码已实现。** 实际 pinned IPC 固定实例/筛选，交付前再核对，最多2000项/2MiB；捕获恢复账本最多32项/64KiB，只存元数据，未完成会话变为中断/结果未知，不重放。损坏账本阻止新捕获；清理只影响捕获及其账本。真实 IPC、持久化和浏览器测试由同一 PR 验证记录支持；手机后台、磁盘不足、系统文件/分享仍需实际验收。

## CLIENT-04

**插件宿主执行与页面通道：客户端独立边界继续实施。** 2026-10-19 从 `2b411b6` 接通[移动前台宿主、设备准入和系统包审阅](docs/gui/mobile-plugin-foreground-v0.0.3.md)；当前提交的 Apple SDK/Simulator 检查仍待 CI，不借用前批结果。QuickJS 经宿主所有的有界工作线程运行，保留原授权 epoch 与单组件 busy lease；独立排队期限不延长原执行时限、授权或 SDK 预算。停用、撤权、页面关闭、实例/材料变化和宿主退出拒绝旧启动与晚到交付。在途原生 SDK 可能继续 drain，不声称已撤回。此容器不提供原生崩溃或进程/OOM隔离，要求 process isolation 的组件仍拒绝。

签名管理页由宿主原子绑定 HTML 摘要、安装材料和组件 review；opaque 文档 token 只在宿主侧，未知组件 ID 不回退。固定 iframe/channel/代次，后台关闭，前台重建，宿主 frame-src 拒绝 URL 导航；SDK 副作用仍使用原 review admission，文件选择前验证能力，返回后再次核对。VM 无法通过自填路径取得系统选择许可，正式 picker/opaque 票据接入前拒绝 VM 文件读写；管理页保留真实宿主选择流程。原生移动前台宿主已接入共享审阅文档通道；原始路径/未绑定 invoke 仍拒绝。敏感加载尚未接通真实移动运行态，不落普通正文。

解除条件：在选定 Android/iOS 执行/分发形态下验收真实 SDK IO、非 JIT/所需隔离、权限撤销、原生来源通知、敏感运行材料和前后台生命周期。共享 worker 及浏览器/桌面服务测试不算移动插件执行或 A20–A22 全验收；它不是 CORE 业务。

## 本批独立客户端安全与交互闭环

2026-10-15 从 `1433527` 继续：四设置表单的草稿离开/提交保护；日志固定筛选的安全
复制、保存和分享；IPC复制的明确元数据边界；旧桌面诊断出口改为实际v3预览后的单次
固定计划。任务历史与当前DNS/路由结果分开，恢复只接纳当前活动事实，A→B→A由客户端
真实 `scopeGeneration` 识别；入队/取消另核对本地 snapshot revision。诊断/日志计划也
固定该代次，旧计划不能因配置切回相同内容重新执行。没有改变 Zero/core 或后置原生装配。
具体代码与验证见[工具任务边界](docs/gui/tool-job-scope-v0.0.3.md)和[固定日志选择](docs/gui/filtered-log-export-v0.0.3.md)。

## CLIENT-05

**已实现客户端链路。** 端点详情及更多菜单进入共用
`TrafficWorkspace`，固定来源配置、实例、revision、endpoint generation 与客户端
`scopeGeneration`；只定位实际 registry 资源。旧式端点仅可定位其唯一已声明的真实
入站/出站角色，并说明不是端点总计；缺失、过期、不安全身份或不支持均给出原因。
返回前重新读取宿主与端点目录，保留原详情模式；对象替换、配置 ABA 和晚回执不能
恢复旧详情。端点来源的统计清空确认也重新核对原目标，不能重选或重放。
隐藏页面保留状态但不轮询或改写当前统计浏览条件；共享事件通道继续保留。
源码与浏览器回归不代表手机端点或统计真机验收，也不提供远端新增原子保证。
契约及针对性检查见[端点统计导航](docs/gui/endpoint-statistics-navigation-v0.0.3.md)。

## CLIENT-06

**已实现客户端分页。** 使用固定 zero-api 的真实
offset/limit/next_offset，每页最多500条，支持前后页、末项、当前页刷新与原失败页重试。
搜索明确只筛选本页，页面之间是实时读取；失败保留带时间的原页并使控制失效。
后续页和确认固定配置、实例、revision 与客户端 scopeGeneration，变化或晚回执不能
继续旧控制；分页、计数和无损 u64 均校验，越界响应不截断。
128个物理目标的关闭屏障现以1MiB有界私有元资料账本持久保留；关闭前跨进程锁定并
确认原子存盘，重启不恢复旧权限或重放。只有原目标权威结束事实或明确拒绝才协调；
列表缺行/closed行、坏回执、未知错误、配置ABA和过期操作token均不释放后来记录。
随机hostSessionId使新宿主的相同本地计数也拒绝旧页面；损坏、丢失已初始化账本或IO
失败暂停新关闭，实际列表仍可读。原生移动关闭与真实平台文件系统仍未验收；旧版未记账
请求和恶意本地回滚不能重建/检测。契约见[Packet分页](docs/gui/packet-route-pagination-v0.0.3.md)。

## CLIENT-07

**已实现独立本地保存、草稿冲突及来源归属保护。** 普通工作区使用真实 Zero schema 与
引用校验，JSON文本保留u64和未呈现字段；插件托管/受保护来源在返回正文前拒绝普通编辑。
纯本地保存不读IPC、不重定向代理、不切换TUN，SQLite发布固定来源/关系、AppConfig、规则
及客户端代次；成功只表示“已保存，待应用”。脏草稿和晚回执不会覆盖新编辑，明确放弃后
才刷新。运行计划固定实际原实例/revision、候选与捕获参数；原连接提交并保留真实回执，
未知或本地发布未完成阻止同profile的新旧计划重放。

**已实现原操作持久恢复及明确本地完成入口。** schema5在副作用前保存普通单目标
候选和原操作阶段，真实receipt先持久确认，再分SQLite关系/AppConfig确认/完成墓碑；
最多32pending、16MiB单候选、64MiB逻辑账本、32完成元资料，候选不进日志/诊断/迁移。
预检固定持久关系epoch和随机宿主session，A→B→A或本地计数重置不复活旧计划。进程
恢复只恢复屏障及原事实；用户明确检查原ID后，以当前scope/session、原真实运行
实例/revision和全部材料摘要取得短期单次许可，只完成原候选本地发布，不apply/TUN/
retarget/下载。正常apply晚回执保留原binding；只有新的明确恢复检查可重新授权本地完成。
完成精确退役原内存屏障，失败/未知不覆盖新源，也不声称回滚。时间回退保留原receipt
事实；票据用单调时间到期。契约见[工作区本地保存](docs/gui/config-workspace-local-v0.0.3.md)。

**剩余独立/协调边界。** 未持久保存真实receipt的未知操作、只有prepared阶段的中断、
来源/epoch/运行身份已变化的历史记录仍只读，不自动清账、重放或猜确认；旧版纯内存
记录无法重建。AppConfig/规则正式写入现使用独立持久文件身份、单调epoch和跨进程锁；
工作区、普通订阅及端点恢复固定原tokens，合作writer的同值写和A→B→A都会使旧许可失效。
旧无tokens记录仅保留历史；首次登记前、旧客户端或外部绕协议同值写、恶意全量回滚
不在证明范围。只有原owned写入意图及实际主备文件吻合才可明确补确认元资料，不重写正文。
坏工作区/Packet账本已有独立有界只读诊断，异常/WAL/锁竞争保留原文件及未知屏障；
没有归档、删除、清账或自动修复。更一般历史管理、原生运行服务恢复仍需继续实施；
需要原操作权威结果时由CORE-03契约协调，不能猜接口。
契约见[只读存储检查](docs/gui/recovery-storage-inspection-v0.0.3.md)。

**工作区冻结材料重启仍可独立继续实施。** 需要重启的变更保留影响与明确不可用原因，
本批没有完成A11的完整重启应用。现有普通启动/监控器会重读当前材料，不能证明原计划
候选生效；固定core拒绝live apply修改`api.control`、`runtime.principal_quota_state_path`。
后续需客户端冻结候选、安全启动材料通道、PID/readiness、原回执、CAS与watchdog窗口。
不能把普通重启后热应用冒充启动字段已应用，也不能将所有重启归为CORE-01/02阻塞。
只读复核还确认：普通start总准备空bootstrap，readiness之后会重读当前活动材料；
专用frozen-start必须持久pending并协调watchdog/自动启动。安全通道不能复用现有
`write_core_config`的普通`fs::write`，仍需私有排他文件、Unix权限/Windows ACL和清理策略。
固定core启动会预取URL规则缓存，且配置父目录影响相对凭据/状态/规则路径；冻结JSON和
typed规则摘要不等于冻结实际依赖。须封存依赖并保留路径语义，或明确拒绝未封存范围。
这些是客户端独立实现前的具体安全边界；受保护材料与手机provider仍需各自适配。
具体启动顺序、配置快照缺失字段和建议的实际Proxy启动回执范围见
[冻结启动边界](docs/gui/frozen-startup-boundary-v0.0.3.md)。PID/health或客户端回显摘要
不能确认启动字段；客户端私有材料、依赖封存及watchdog工作仍可独立继续。
内核管理普通重启独立保留；没有购买账号、创建签名材料或改变原生后置范围。

本批准确提交的全量验证与CI证据见同一Draft PR；浏览器、合成IPC及本地SQLite检查不
替代真实手机运行、远端原子前置条件或进程恢复验收。

## CLIENT-08

**普通手工配置无损文本编辑：本批有限实现。** 基线`19682c7`普通ProfilesTab仍将正文
`JSON.parse`并以Value提交，编辑又从列表DTO stringify；最大u64会被舍入。新入口使用Rust
sourceJson快照、原生校验/格式化和固定来源本地CAS，保存与明确启用分开，不另建运行事务。
托管/受保护来源无正文，普通订阅来源只读；path-only材料不自动读路径。规则集/GeoIP会在
固定native parse中触发文件IO，未获依赖授权前明确拒绝并保留草稿，不删字段或降级假校验。
是否已通过及实际剩余以[文本编辑契约](docs/gui/profile-editor-lossless-v0.0.3.md)和同PR准确提交证据为准。
这不修复历史已舍入数据，也不等于A08所有来源/依赖/运行或真实文件提供者已验收。

## CLIENT-09

**其余明确的独立用户功能，不能归为全部已后置或core阻塞。** 全部A01–A24证据及边界见
[完成度矩阵](docs/gui/mobile-client-completion-matrix-v0.0.3.md)，避免只在恢复机制上重复实施。

| 范围 | 当前缺口与下一退出条件 |
| --- | --- |
| A10路由 | 本批完成普通来源profile-owned routeRules覆盖CRUD/顺序/启停、固定预览与本地CAS待应用；本批另完成规则集来源/覆盖/旁路/DNS引用清单、明确替代、固定本地CAS迁移与删除；跨配置/托管等不支持范围明确拒绝，授权依赖resolver仍待实现，不编造原生enabled/protocol字段 |
| A18连接 | 本批实现实际宿主清单、明确多选、固定预览与六态逐项结果，见Flow契约；未知不重放。仍有远端原子前置条件、进程恢复及真机验收边界 |
| A19诊断 | 本批实现实际路由评估及独立dns_cache任务；明确缺失、未命中、未启用和读取失败。真实采样及设备后台/系统交付仍待验收 |
| A04定位 | 本批实现公开ID/binding与真实NodeScreen选择链定位、保留底栏栈/搜索/草稿的guarded visit及来源返回；规则/直连/未确认分别表达。原生运行通道与真机恢复仍未完成 |
| A12/A13 | 统一MTU、完整u64 UDP、既有监听IP/端口及LAN影响的固定预览/本地CAS待应用保持。2026-10-16补DNS重命名真实引用扫描、固定预览/明确确认、完整Value与u64保留及原操作未知屏障，见[迁移契约](docs/gui/reference-migrations-v0.0.3.md)。已有全局DNS与单份普通来源可本地保存待应用；跨配置来源晋升、托管/路径来源、带ruleSetReferences来源晋升明确拒绝，保留正文与草稿。跨配置/托管来源迁移、来源投影整合及其它协议/条件字段仍未完成；原生/真机边界不变 |
| A20–A21插件 | 移动容器/SDK/分发/敏感加载仍是客户端平台工作；本批卸载管理不开放这些执行能力 |
| A22卸载 | 新增固定安装/发布者/namespace资源与引用预览、主应用确认、原操作逐项回执/剩余/未知/手动状态及目录缺行后可发现的记录入口，见[契约](docs/gui/mobile-plugin-uninstall-v0.0.3.md)。仅非活动且可证明安全范围执行；在用配置须先明确切换/停止后重新预览，不复用旧自动首项。完整运行替代/停止、移动执行及真机仍未验收；只用隔离fixture，不实际卸载用户资源 |
| A23/本地数据 | 已有宿主前台版本目录due执行、已保存客户端渠道、manual/auto去重、真实200/304及偏好原回执确认；不是下载安装。2026-10-19 补充三类固定记录文件的真实容量、后端现行保留预算、未知/过期采样和清理后重读，见[本地容量](docs/gui/local-data-capacity-v0.0.3.md)。安装器/OS后台调度、设备验收、单条/导出目录逐项清理和统一可配置预算仍未完成 |

用户明确后置的是native VPN、core接入与宿主生命周期装配；不把这个决定扩大为所有移动
插件、安装器或用户管理功能已获后置。平台必须允许的发行路径、设备及签名条件仍单独说明。

## PLATFORM-01

**平台与真机条件。** iOS 主应用/extension/App Group/允许 entitlement、移动 Host 采样器、插件执行与 SDK/敏感材料边界、发行安装路径和缺少许可材料仍按验收账本推进；平台不允许与尚未适配分别标注。没有付费 Apple 账号，不创建签名凭据，不合并或发布。

## CLIENT-10

**iOS 主应用 UI 宿主与调试入口。** 新增锁定 Tauri CLI 标准工程生成、无签名身份的完整 Simulator App/unsigned iPhoneOS App 构建、Debug 真实 WKWebView/IPC 烟测及 [本机调试指南](docs/gui/ios-ui-host-v0.0.3.md)。Simulator 工具链 ad-hoc 与设备 unsigned 分开记录。工程与实际执行证据由对应 commit 的 macOS CI 产物确认，不再拿 NativeIo 单库代替完整应用。最低 iOS 16.4。2026-10-19 补固定包内许可 reader、实际 SwiftPM/Cargo/Vite 输入组合与完整 App 二次构建/资产核对；每个提交仍须取得真实 SDK 和 Simulator IPC 证据，已有材料与逐包最终链接证明缺口不因脚本存在而解除。

**继续后置：** Packet Tunnel extension、App Group、内核/原生 VPN 生命周期及 CORE-01/02/05；签名、设备配对、安装和真实 iPhone UI/文件/生命周期验收由用户本机执行。云端不创建账号凭据、证书、私钥或 provisioning，不发布或合并。

解除条件与验收：Galaxy S8/API24 及 iPhone 的实际设备和允许环境；前后台/锁屏/撤权/服务重建/网络切换/磁盘不足/文件或分享取消，以及 TCP/UDP/DNS/Fake-IP 与声明协议测试。浏览器、无签名 APK 或 UIKit SDK 库构建不替代这些结果。
