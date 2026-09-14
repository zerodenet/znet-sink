# 客户端插件：设备准入、沙箱与能力授权第一阶段

2026-09-12 后续架构纠正：以 [客户端统一入口、管理与执行管道](./unified-capability-plan.md) 为最新实施方案。插件业务由插件实现，所有内核操作经客户端代理；普通与受保护来源共用一条执行链。本文旧业务权限名、插件直连/解密所有权和独立执行路径仅作历史候选，冲突时以上述方案为准。

2026-09-12 后续：组件准入已接共同 Manager 注册表，重复装载共享授权/运行占用，升级、停用、卸载和客户端退出的后端生命周期规则已实现并由 VM 消费。授权确认绑定注册代次，旧确认不能跨卸载重装生效；GUI 加载和用户授权界面已继续接入，已开放自身信息和按 origin 授权的网络 GET/通用请求；配置及受保护材料 SDK 尚未接通。详见 [迁移进度](./capability-migration.md)。

2026-09-11。状态：本地实验运行器已实现；生产客户端尚未加载第三方插件。此阶段由用户明确启动，接续 #42 的客户端能力，不修改 #42 的验收范围。旧 [插件候选](./plugins.md) 中未在本文确认的 SDK、界面和执行器仍是候选。

2026-09-12：已继续实现 [目录读取、签名发布包与本地安装链](./plugin-distribution.md)，包括 `targets: "any"`。下文为第一阶段记录，发布格式/安装流程的最新状态以该文为准；生产 GUI 和系统沙箱仍未接入。

2026-09-12 用户进一步明确：后续以 [ZBoard 联动主线](./zboard-plugin-target.md) 为目标，优先业务授权和资源策略，独立 worker/OS 沙箱不再作为前置条件。下文阶段顺序保留为历史记录。

## 产品范围

ZNet Sink 是跨平台网络工作台。插件用于扩展配置来源、有限观测分析和客户端操作；不在 Zero 内核注册协议，不创建另一套流量事实，也不把已有内部工具重新包装为插件交付。首个业务插件仍以 ZBoard 面板接入与受保护配置为目标。当前记录摘要样例只验证隔离与授权边界，不能算完成面板插件或新的流量工具。

配置来源插件、离线分析组件、未来位于网络执行路径的处理脚本应有不同角色和能力。实时正文处理和改写需要另行确认 Zero 执行点及延迟/故障契约，本阶段不触及。现有 OnPhase 同步钩子不作为第三方启动入口。

## 分发：共用中央目录，宿主分别执行

已核实中央仓库 `zerodenet/plugins` 的 main 为 `07ccd9600951814b8de7d2f2ef942b93c54def4d`。本地 `/Volumes/tool/plugins` 工作树仍在旧提交且有未提交修改，未更新或修改它。以下依据远端当前文件：

- [目录 v2 契约](https://github.com/zerodenet/plugins/blob/07ccd9600951814b8de7d2f2ef942b93c54def4d/docs/registry-format.md)：目录登记插件 ID、发布仓库、发布者公钥、信息、release_source，以及 surfaces/capabilities 上限。
- [ZNet Sink 目录](https://github.com/zerodenet/plugins/blob/07ccd9600951814b8de7d2f2ef942b93c54def4d/catalogs/znet-sink.json)：schema_version=2，当前 plugins 为空。中央设施存在不等于已有客户端插件可安装。

客户端读取目录，再读取发布者 GitHub Releases 的元数据和资源；日常发版不改中央目录。目录准入、包签名和用户授权是三个独立条件。签名正确不能越过目录能力上限，也不能自动获得本机权限。

拟定安装链：目录身份/公钥 → 发布元数据 → 宿主、版本、设备、运行时和隔离要求 → 下载大小/摘要/签名 → 安全解包（路径穿越、符号链接、数量/解压容量限制）→ 原子安装 → 默认停用 → 用户按组件授权 → 运行。更新导致摘要变化时旧授权失效；保留上一版本用于回滚，禁止自动继承新增权限。目录撤销、能力缩减与本地停用须终止相应任务。具体包格式、签名消息规范、信任根更新和离线目录过期策略在安装阶段固定，实验 manifest 不作为发布格式承诺。

ZBoard 的服务器插件进程、gRPC/页面入口和安装包不能直接作为客户端包执行。中央目录按 host 区分；同一发布仓库可以产出不同 host 的包，各自声明能力和设备。第一阶段不复制服务器插件市场的运行时。

## 设备与隔离

发布包按“插件 ID → 组件/角色 → 设备变体”组织。组件分别声明必需/可选能力与隔离要求；必需组件不兼容时对应插件功能不可启用，可选组件不兼容时明确显示该功能不可用。设备变体不能隐式继承另一变体的权限；共享 JS 资源可以复用，但授权和运行状态仍按组件与当前设备独立维护。当前实验 manifest 仅描述一个组件，尚非完整发布包格式。

设备准入同时匹配 OS、架构、设备形态、客户端版本、运行时 API、可用能力和最低隔离级别。不能因为 JS 文件相同就宣称所有设备均支持；不能只用 desktop/mobile 一个字段代表完整兼容性。

| 设备 | 目标执行方式 | 当前状态 |
| --- | --- | --- |
| macOS / Windows / Linux 桌面 | 独立受限 worker + 宿主能力代理；各系统实现自己的文件、网络与进程限制 | 当前仅在本机 macOS 实测进程内 JS VM；worker/OS 隔离未实现 |
| Android 手机 / 平板 | 优先验证 isolatedProcess Service + 能力代理；明确后台、VPN 生命周期和系统资源预算 | 可描述设备契约，未编译/验证运行时，不开放安装 |
| iOS 手机 / 平板 | 按发行规则与扩展生命周期决定可用组件；达不到要求的组件不可用 | 已核对规则但未验证本产品适用性，不承诺通用第三方 JS 插件支持 |

平台依据：Android 的 [isolatedProcess Service](https://developer.android.com/guide/topics/manifest/service-element) 自身无权限，通过 Service API 与宿主通信，适合作为待验证执行域；这不代表现有客户端已经具备该服务。Apple [审核规则 4.7](https://developer.apple.com/app-store/review/guidelines/#mini-apps-mini-games-streaming-games-chatbots-plug-ins-and-game-emulators) 涵盖部分插件软件，但 4.7.2 对暴露原生平台 API 有事先许可要求，4.7.3 要求分享数据/隐私权限时取得明确同意。因此本方案推断 iOS 必须单独验证能力桥接与发行路径，不能把“可运行 JS”当成上架可行性结论。

平台支持清单由宿主维护，不由包自报。移动端的发行政策和运行时选型必须在该端实施前查证。`minimum_isolation=process` 的组件在当前 VM 实验器被拒绝；不能降级到 VM 后照常运行。即便桌面 worker 可启动，也只有加入系统资源/IO 限制后才称为 OS 沙箱。

## 已落地的本地边界

实现位于 `src-tauri/crates/plugin-sandbox`，与 Tauri、client-core、engine-client、Zero 无依赖。未接入 GUI 自动加载或生产启动流程；workspace 全量测试会覆盖它。

- 严格解析实验 manifest：host、版本范围、OS/架构/形态、API、最低隔离级别、源码摘要和资源预算；拒绝未知字段、未知能力、重复请求和无效范围。
- 组件身份绑定 manifest 原始字节与源码 SHA-256。摘要只识别内容，**不认证发布者**；CLI 明确只用于用户主动选择的本地样例。
- 宿主先按独立能力上限准入，再显式启用/授权。授权绑定组件摘要、具体资源范围和有效期，最长一小时；默认没有授权。必需权限缺失则不启动。
- 同一 Authority 同时只允许一次执行，没有等待队列。撤销或替换授权递增代次，旧任务的回调、中断检查和最终结果交付均校验；超时/取消/失效不交付结果。授权当前只在内存，重启全部消失。
- 每次调用创建独立 QuickJS VM。只安装基本 JS、JSON、求值与异步函数原型初始化所需的 Promise 内建，移除 guest eval/Function 和各类函数构造器；没有 Node、DOM、Tauri、文件、网络、模块加载器、定时器或后台任务调度器。插件通过唯一 `hostCall` 请求类型化能力；未调度 Promise jobs，拒绝 Promise 结果，未实现异步 SDK。
- 只实现 `plugin.self.read`（self 范围）和 `records.summary.read`（精确 selection ID 范围）。摘要仅含记录数/上传字节/下载字节，由宿主预先投影；没有连接正文、密钥、配置、原始 IPC 或数据库句柄。
- 最大 VM 堆 32 MiB、栈 512 KiB、执行/结果序列化 2 秒、调用 32 次、源码 256 KiB、manifest 16 KiB、单请求 1 KiB、输出 64 KiB。更低预算允许；超额拒绝。VM 堆预算不是整个 OS 进程的 RSS 上限。引擎 OOM/栈错误当前以 GuestException/InvalidOutput 失败，尚无精确分类诊断。

运行时固定 `rquickjs = 0.13.0`，未启用自定义 allocator 或模块 loader。资源控制使用 [rquickjs Runtime API](https://docs.rs/rquickjs/0.13.0/rquickjs/runtime/struct.Runtime.html)。本阶段信任宿主 Rust 与 VM 引擎实现，未防御引擎原生漏洞；不以纯 JS API 限制替代进程级隔离。因此不接入密码、配置解密和第三方下载包。

## 运行与验收

在 GUI 仓库根目录执行：

```sh
cargo test --manifest-path src-tauri/Cargo.toml -p znet-plugin-sandbox --locked
cargo run --manifest-path src-tauri/Cargo.toml -p znet-plugin-sandbox --bin znet-plugin-lab --locked -- examples/plugins/record-summary/manifest.json examples/plugins/record-summary/main.js examples/plugins/record-summary/grants.json examples/plugins/record-summary/summary.json
```

第二条输出 `records: 3, total_bytes: 1000`。将 `grants.json` 换为 `denied.json` 应返回 PermissionDenied 和非零退出码。输入为仓库内虚构样例，尚未接入连接页面的实际选择。

测试覆盖正常执行、默认拒绝、能力上限、错误范围、宿主选择范围、未知能力、吞掉越权异常后仍不交付结果、危险全局对象/函数构造器、动态 import、无限循环、序列化死循环、内存/栈/输出/调用预算、摘要更新失效、单组件并发、执行中撤销、过期、取消、设备/版本/隔离不符和不同执行间不共享全局变量。测试通过只证明上述路径，不证明沙箱无逃逸漏洞。

## 本地验证记录（2026-09-11）

- 新增模块：13 个测试通过，其中一个启动真实 CLI，验证授权样例输出与拒绝授权时的退出行为。
- `cargo clippy --manifest-path src-tauri/Cargo.toml -p znet-plugin-sandbox --all-targets --locked -- -D warnings` 通过；workspace rustfmt 检查通过。
- `cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked -j 1` 未通过：GUI 包 428 通过、1 失败、4 忽略。未修改的 `runtime_host::readiness::tests::healthy_ipc_for_another_pid_is_rejected` 未得到预期的 different process 错误，fixture 收到 EOF；该测试单独复测仍失败，具体原因未确认。本批没有修改其实现或放宽断言。
- 随后单独执行 client-core、engine-client 与 plugin-sandbox 三个包的测试（49 通过），以及 GUI 集成测试（96 通过、1 忽略），均通过；补齐被失败中断的测试目标，但不将整轮回归标记为绿色。
- 当前证据来自本机 macOS；未做 Windows/Linux/Android/iOS 运行验证，没有生产客户端集成验收。

## 下一阶段顺序与完成条件

1. **桌面受限 worker 与宿主授权入口。** 将当前 VM 放到独立 worker，限制原生文件/网络/子进程能力；能力代理留在可信端，每个请求和响应校验身份与授权代次。宿主统一拥有每组件实例，不能创建第二个 Authority 绕过并发上限。增加启用、查看权限和撤销 UI，撤销真实运行任务；worker 崩溃/失联由宿主回收。先完成本机端到端，再逐端宣告支持。任务进程重启后标为中断，不自动重放有副作用操作。
2. **目录 v2 安装闭环。** 固定签名包契约与发布工具，实现上述安装链、能力变化提示、升级回滚和设备不可用说明。验证篡改包、错 host/设备、公钥不符、能力超限、解包攻击与断电恢复；此时才开放第三方包安装。保持中央仓库为唯一准入目录。
3. **首个实际业务插件。** 按 [受保护配置](./protected-config.md) 接入 ZBoard。先补按 origin/账号绑定的网络能力及秘密通道；完整审计配置数据库、临时文件、导出、IPC/debug、错误日志和崩溃路径，确保解密明文不回流宿主业务层。密钥由面板管理，在合格执行域内使用。现有 Zero 接口不足时明确单列内核需求，不偷偷修改内核或把“能传配置”当成保密闭环。
4. **移动端适配。** 复用能力契约、目录与签名包；按 Android/iOS 实际限制决定可用角色、组件资源和后台行为。没有合格执行域的受保护组件显示不可用，保留可支持的非敏感功能。分别验收后再发布兼容声明。

下一批不继续扩展摘要分析工具，也不做自由脚本编辑器/插件商城页面。优先补桌面隔离与真实授权入口，才能安全承载后续安装和业务插件。
