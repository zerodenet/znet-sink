# v0.0.3 移动客户端适配验证记录

2026-10-13。客户端基线 `develop@fd198ba52711f018334860b8a6704546ccd349fd`，
工作分支 `feat/mobile-v0.0.3`，开发版本 `0.0.3-dev.202610131010`。
本次仅在云端实现和验证，没有在用户 Mac 修改、构建或测试。

本记录以交接的完整移动 PRD V3 为基准；HTML 只提供布局参考，没有复制其
模拟数据、事务定时器或假成功。此次是持续更新的客户端适配 draft，**不是完整移动
VPN 交付**。下列“通过”只指对应代码检查或替代 IO 的测试，不代表手机验收。

本记录保留基础阶段与第二阶段的历史证据。2026-10-13 后续诊断/许可/平台投影
整合的当前范围及逐项剩余条件见 [移动验收账本](../gui/mobile-acceptance-ledger-v0.0.3.md)、
[有界诊断](../gui/bounded-mobile-diagnostics-v0.0.3.md) 和
[构建依赖许可目录](../gui/dependency-license-inventory-v0.0.3.md)。最新 commit 的
CI 链接、计数与最终 APK 收据在同一 draft PR 描述和交付报告核对，不借历史成功替代。

## 2026-10-15：锁定版本的原始许可材料

本批从 `8c043cd53a831ac27237fbae9c46f21aeb452377` 继续。新增 npm/Cargo/Maven/Swift
精确身份的原文缓存，保存原始版权、NOTICE、标准条款来源及字节摘要；字体、旗帜与 CSS
按实际构建输入/输出记录，未安装或未使用的包不因 manifest 出现就声明打包。
Cargo release、Android merge 与最终 APK receipt 检查材料缓存新鲜度；自有 crate
明确引用实际 MPL 文件。具体缺失和共同条款边界见 [许可目录](../gui/dependency-license-inventory-v0.0.3.md)。

云端 `pnpm verify:frontend` 通过，Svelte/TypeScript 为 0 错误/0 警告；最终许可专项
31 项 Node 测试、12 项 Python 测试、Swift fixture 与原有清单断言通过。
真实 Rust build-hook fixture 验证匹配清单、过期缓存/本地材料、非法路径、正文篡改及引用拒绝，
并使用实际 Cargo build-dependency 版本，避免误取另一个 sha2 版本。
Rust fmt、workspace all-targets/all-features check、全特性 tests 与 clippy 通过；
测试 1116 通过、0 失败、7 ignored，已有警告仍保留。

新的真实 Gradle release collect 已通过，68 Maven 与 6 本地项目的实际归档/POM 摘要
和原文材料可合并。许可批次 `0d20849` 与 SDK 修复 `a55e84b` 的云端无签名 APK
已实际构建并通过 final receipt：50,117,939 字节、arm64-v8a、min API24 / target36、
`org.zerodenet.znetsink`，Native IO 已包含；500 项、484 份唯一正文、4 项明确限制。
APK SHA-256 为 `4dafb7a8b2c8f634cdcc313ed8b2d5f59a111951828df06977e472f3671818fa`。
后续日志源码改动不借此产物宣称已打包，其最终 commit 仍由对应 CI 重新构建。

`a55e84b` 的 Native documents CI 已成功完成 Android library、Swift policy 8 项测试
与真实无签名 UIKit Simulator SDK 构建（arm64/x86_64）。SDK 清单各 9 项、3 项明确缺口，
没有称作 iPhone app/provider 产物。之前 `0d20849` 的 Apple 测试错误依赖 runner SDK
自动发现，导致实际 UIKit 步骤跳过；修复将显式 metadata 与隐式发现隔离，并校验同时
给出的 SDK root 原始字节匹配，补入跨平台执行的 Darwin 分支 fixture。
没有 iPhone、Galaxy S8、VPN 服务/扩展和设备网络验收。

## 2026-10-15：宿主类型化的诊断日志正文

诊断文件使用 `znet.mobile-diagnostics.v3`。真实进程、调度、节点、测速、旧连接清理
和插件宿主路径通过不含自由字符串/任意 JSON 的封闭事件提供固定 `safeMessage`；原消息、
动态字段及未确认的身份仍排除。普通 RPC、Core 和 guest 日志不授予分类；来源不符、
旧 marker、未知或畸形持久分类只保留可读元数据，不升级为当前运行事实。

插件分类来自实际 Run/Uninstall、SDK typed Method 或调度调用点；generic invoke 固定
ComponentAction，名称碰巧叫 `sdk.SubscriptionApply` 或 `uninstall` 也不会借用其语义。
规则依赖缺产物只记录 ArtifactUnavailable；未使用的 provider 更新失败仍可通过真实配置
注入校验，不声称一定阻止配置应用。测速本地失败不标为远端不可达。

最终 focused 39 项、JSONL store 11 项及既有被引用 provider 缺失拒绝回归 1 项通过。
覆盖真实通用 invoke 的 8 个同名碰撞、6 个 SDK 类型映射、未使用 provider、RPC/普通入口
伪造、混合秘密 canary、source 冲突、JSONL 轮转重开与固定 preview/take。独立只读复核
已核对调用点、SDK desktop 条件编译和两项事实问题的修复。最终 Rust fmt、workspace
all-targets/all-features check、全特性 tests、clippy 及实际 license build-hook fixture
通过；tests 为 1129 通过、0 失败、7 ignored（45 组）。完整前端验证、后续权限/归属和
115 项移动语义回归通过。准确提交 CI 在交付前另外核对；旧 APK 仅证明前一许可批次，
不作为这批日志源码的手机验收。

分类持久化沿用应用私有日志；它不提供对直接改磁盘主体的密码学真实性，也不用于控制
内核、恢复旧任务或判断当前 VPN 健康。其他自由正文保持省略，仍有真机后台/分享等验收缺口。

## CI 后续：运行所有者回执与身份发布顺序

`00462b0` 的实际 isolated Proxy CI 揭示命令结果先唤醒调用方、再发布新 revision 的竞态；
立即读取身份再查询时可能得到 StaleIdentity。客户端 owner 改为先发布实际操作后的身份，
再交付原始成功或失败回执；保留正在关闭阶段、精确 CAS、原操作结果和不重放约束。
同步回执 waker 的确定性测试在旧实现失败、修复后通过；新增真实 Proxy 的 32 次模式事务
检查新身份、运行 revision、旧身份拒绝及失败命令不改变已确认模式。原 CI 用例另外重复
200 次通过。isolated workspace fmt、all-targets/all-features check、7 项全特性测试和
clippy（拒绝 warnings）通过；core 与锁文件未修改。原生 VPN 装配仍未启用。

## 本次代码

- 原生 Android/iOS 或窄窗口采用四固定底栏。高级管理由移动投影保留，不修改
  用户桌面 Lite/Pro 偏好。二级页保存来源，各底栏已访问的页面保留搜索、展开、
  草稿与滚动；工作区、配置和订阅编辑离开时确认未保存内容。
- 概览只有一个首屏统计入口，定位实际全局 Flow。统计工作区复用应用共享会话，
  无损 u64 累计与本次采样分开；当前速率未知显示 `—`，不同实例、配置、epoch
  和观测缺口重新建立基线。后台暂停 UI 查询，保留公共事件通道；前台重新发现。
- 节点页按配置顺序展示全部组，多组展开，搜索不覆盖用户展开记录；区分直接成员
  selector 与自动/其他组。组或单节点测速固定目标，停止保留 draining 事实，
  重试只取原任务失败项。节点、测速与模式调用增加客户端作用域前置条件。
- 配置与订阅为两个分段，路由、规则集和原生工作区保留二级入口。离线本地选中
  与运行态确认分开。沿用 Rust 配置、订阅、DNS、规则、持久化和应用链，未新增
  “移动 JSON schema”。未知字段、来源归属和原有权限仍由现有领域决定。
- 端点区分“清除运行覆盖”和“恢复来源”。后者移除客户端覆盖之前核对普通来源、
  已确认配置回执、资源身份及版本，先预览影响，再协调实际配置与端点回执；
  本地待应用、运行确认和持久化分别记录。缺少可信绑定或运行桥时不能宣称恢复。
- 统计只呈现所选范围实际提供指标的平面；没有虚构 Host identity。新 Packet
  页面使用 `zero-api` DTO、真实查询与关闭命令、固定目标确认、profile/实例/
  revision 校验、无损计数和回执核对；未知结果不会自动重放。
- 移动 Tauri 装配、私有容器和权限独立于桌面。没有移动运行桥时，拒绝桌面
  进程、代理、TUN、安装器、原始 IPC 和运行操作，不伪造可转发内核实例。
  发行页分别展示默认随包策略与实际缺失的移动内核产物。
- 新增宿主持有的 Android/iOS 系统文档插件，按用途限制读取和导出预算，原生
  路径/URI 不进入界面。普通配置导出需确认凭据风险；托管材料拒绝导出；取消、
  分享已接受及结果未知分别显示。诊断导出使用固定、有界脱敏快照，含版本/平台、
  安全日志分类、真实任务/捕获状态及具有可信来源的公共计量事实，原始正文默认排除。
- 设置迁移白名单、覆盖范围预览、原生 schema 与 DNS 全引用校验、版本/实例/
  来源前置条件及原子本地保存已接入；本地保存和运行应用分开，无自动 VPN 启用。
- 删除配置先固定对象、订阅关系和版本，明确替代或停止；没有自动选择首项。
  原生移动当前配置删除仍因 VPN 状态未知受阻，独立非当前配置可本地删除。

## 平台及浏览器边界

Android 最低系统明确为 7.0/API 24，Galaxy S8 仍是必须真机验证的设备。
API 下限、Rust target 编译和 320px 浏览器检查不能替代该设备的包安装及运行。

当前技术基线使用 Tailwind 4，前端构建显式以 Chrome 111/Safari 16.4 为目标。
系统版本与 WebView 引擎版本需分别检查；不声称 Android 7 原始 WebView 已兼容。
[Tailwind 官方兼容基线](https://tailwindcss.com/docs/compatibility)、
[Tauri WebView 说明](https://v2.tauri.app/reference/webview-versions/)。
iOS 主应用最低版本声明为 16.4，与前端基线一致；Native IO 库下限仍为 15.0。
已有 Xcode 无签名 Simulator SDK library 构建，尚无主宿主/扩展构建，
没有付费开发者账号、签名凭据、安装或真实 iPhone VPN 验收。

完整手机运行仍需 M01–M04。M01 生命周期和 M04 控制/事件已有公开 Rust 接口，
客户端服务所有权、随包打包、鉴权与持久恢复可独立推进；跨仓库缺口主要为
M02 外部 Android FD/iOS packetFlow 与 M03 全部外层 protect/bind/网络代次。
[跨仓库 API 清单与退出测试](../gui/mobile-platform-v0.0.3.md) 已给出 crate/trait 层次，
core 工作树只读且未推送。默认随包策略尚未落实为移动内核构建产物。

## 验证

诊断/许可/Host/发行检查整合后的最终本地源码验证（2026-10-13）：
workspace all-features **997 passed / 0 failed / 7 ignored**；fmt、all-targets/all-features
check 与 clippy 通过，既有警告保留。`pnpm verify:frontend` 完整通过，71 项移动
语义测试、17 项许可来源/材料/target/产物断言、0 错误/0 警告及生产构建；
完整 Chromium **203 passed**。Rust build hook 的实际编译 fixture 和 Swift pin/SDK/多架构
来源 fixture 通过；后者不是 UIKit 实际编译替代品。日志前缀 `/tmp/mobile-phase3-source-final-`
及 `/tmp/mobile-phase3-final-`。最新四组 CI 尚需按本轮新 commit 单独核对。

本地实际 Tauri CLI 无签名 arm64 APK 为阶段证据：47,938,295 字节，API24/target36，
SHA-256 `b37ec4e58ab46915d1382cb9d4bf0febb187094f7b4d833f79cbba5a1f4877b1`。
真实 fixed asset、frontend SHA 和 `libgui_lib.so` 已核对，496 条记录/313 份去重材料/
69 项明确缺口（计数随 toolchain/target 变化）。诊断来源最后修正发生在此构建之后，
因此此包不能称为最终 PR head 产物。最终 head CI 会重建并上传 APK 和外部收据，
不得用该阶段包或旧 commit 的绿灯替代。

| 检查 | 本次结果 | 不能据此推断 |
| --- | --- | --- |
| `pnpm verify:frontend` | 全部现有回归、新移动/Packet 回归、类型检查及生产构建通过 | 手机服务与真实内核转发 |
| `pnpm check` | 0 错误、0 警告 | 系统 API/键盘已在手机验收 |
| Rust fmt、workspace check/all-targets/all-features | 通过 | iOS 宿主/扩展链接与签名 |
| Rust workspace/all-features 测试 | 基础阶段 899；新增管理/文档改动后 955 passed、0 failed、7 ignored | ignored 的真实二进制/平台测试已运行 |
| Rust workspace/all-targets/all-features clippy | 通过，保留既有及测试代码警告 | 严格 `-D warnings` 已通过 |
| 共享 Rust Android/iOS aarch64 target check | 两个共享 crate 均通过 | Tauri/iOS 主宿主或扩展已构建 |
| Android Rust 主宿主 API24 check | NDK r27d/API24 clang，`--lib --target aarch64-linux-android --locked` 通过；40 项未调用桌面代码等警告 | APK、签名、安装、TUN FD、S8 或 VPN 验收 |
| Chromium 浏览器 | 基础阶段 171；新增文档/迁移后全套 175 通过、3 项截图因磁盘不足失败，清理后原 3 项全部通过；另加 5 项实际删除 Dialog、3 项系统返回/页面恢复及 3 项实际关于页测试通过 | Android 原始 WebView、WKWebView 或手机系统返回 |
| WebKit 本地运行 | 未运行：当前云端浏览器下载被网络网关拒绝（403） | iOS/WebKit 验收通过 |
| 原生文档库 | 实际 Android 库编译、11 JUnit 与 AAR 通过；Swift Foundation 8 项通过 | Files/分享、UIKit SDK 或设备验收 |
| Android 无签名应用 | 实际 arm64 Release APK 构建成功；最低 API24，检测无签名；最后源码仍须按 commit CI 复核 | 可安装签名包、Galaxy S8、VPN 服务或随包内核 |

浏览器测试以真实生产组件和明确的替代 IO 执行；测试 fixture 不进入产品模块。
实际 Unix IPC 的 Packet 测试证明查询/关闭 DTO 与回执处理，仍不是网络转发真机证据。
详细云端日志保留于 `/tmp/mobile-frontend-verification.log`、
`/tmp/mobile-backend-tests.log`、`/tmp/mobile-cargo-check.log`、`/tmp/mobile-clippy.log`、
`/tmp/mobile-android-host-check.log`、`/tmp/mobile-browser-full-final.log`。
后续 Rust 修复全套证据为 `/tmp/mobile-final-rust-{fmt,check,tests,clippy}.log`，
升级事务回归为 `/tmp/mobile-upgrade-transaction-regression.log`；
测试 runner 的 Linux PowerShell 语法/C# 编译/参数转发证据为
`/tmp/znet-windows-runner-pwsh-validation.log`；插件示例回归为
`/tmp/mobile-plugin-example-checkout-regression.log`。

## 草稿 PR 与 CI

[Draft PR #46](https://github.com/zerodenet/znet-sink/pull/46) 合入目标为 develop。
首个代码提交 `f18f72e7f7ad10d0708dbcef205434a362de87f8` 的
[移动宿主 CI](https://github.com/zerodenet/znet-sink/actions/runs/38382760624)
中 Android API24 主宿主及 Android/iOS 共享 Rust 三项均成功。
[浏览器 CI](https://github.com/zerodenet/znet-sink/actions/runs/38382760521/job/115246989500)
Chromium/WebKit 共 342 项通过；本地 WebKit 未运行与此 CI 结果分别记录。

同一提交 Windows 客户端测试在启动前返回 `0xc0000139`
(`STATUS_ENTRYPOINT_NOT_FOUND`)，未执行测试，不能计为通过。
后续保留 Windows2025 全部检查，增加依赖入口诊断及隔离缓存的 Windows2022
对照。两套环境均复现相同加载失败；Python 探测已有 v6 Common Controls 激活上下文，
因此没有直接缺失入口，不足以证明 Cargo 测试 EXE 可加载。依赖中的 rfd 启用
`common-controls-v6` 并静态导入 `TaskDialogIndirect`，与
[Tauri 上游的库测试 manifest 缺失问题](https://github.com/tauri-apps/tauri/issues/13419)
吻合。后续测试 runner 在执行前读取 `RT_MANIFEST #1`，只为缺失 manifest 的
Cargo 哈希测试 EXE 用 Windows SDK `mt.exe` 嵌入最小 Common Controls v6 依赖。
现有 manifest 保留，全部测试参数及真实退出码继续传递；发行构建不使用该 runner。
`5187eb2` 的两套 Windows CI 已证实 manifest 原本缺失且完成嵌入，各实际
执行 623 项成功测试；随后暴露同一个升级备份断言失败，不能计为整项通过。
Windows 将“普通文件/receipt.json”的读取错误归类为 NotFound，旧逻辑会忽略
损坏的 `upgrade-*` 条目。后续修复先检查保留前缀的备份类型，并要求持久回执；
缺失或损坏时明确要求恢复，无关普通文件和目录继续兼容。新增两项回归，
事务 13 项及本地 workspace/all-features 899 项通过；`d391bfd2` 的两套 Windows GUI 主测试均通过，后续 workspace 在插件示例
入口摘要校验失败。隔离的 `core.autocrlf=true checkout-index` 重现：示例 JS
变成 CRLF，与 manifest 的 source_sha256 不符。补齐示例目录的 LF 属性后同样
checkout 保留声明摘要，原离线打包/安装/执行测试通过。没有更改摘要或校验逻辑；
基础阶段 head `ded57c37e46c01ab30cc4c09a90c6ca3c2030889` 的 Windows 完整
workspace/readiness 及其他 CI 已于 2026-10-13 12:15 UTC 核对全绿：
[主 CI 14 项](https://github.com/zerodenet/znet-sink/actions/runs/38393241966) 与
[移动 Rust 3 项](https://github.com/zerodenet/znet-sink/actions/runs/38393241982)。
后续设置迁移、文档 IO、删除与来源恢复是新的改动，必须按新提交单独核对 CI。
历史 PR #44 的合并提交 `7bf9337aa6a71f93acce4d9761a523946bf4e2ab`
不是本次 develop 基线的祖先。逐处比对确认相关 LF、备份文件守卫及库测试 manifest
修复未带入，而网络诊断 fixture 已有等价内容。本次按独立补丁修复，不合入 main
发布变化。Windows workspace 保留全部测试并使用 `--no-fail-fast` 收集其余套件
失败；失败仍是非零回执，构建依赖按 runner 隔离缓存。前端 CI 补充已验证的
桌面 shell 回归。

两个镜像名称来自 [GitHub 官方 runner 列表](https://github.com/actions/runner-images/blob/main/README.md)。

## PRD 验收状态

| PRD 范围 | 客户端完成部分 | 尚未通过的部分 |
| --- | --- | --- |
| A01–A02 | 四底栏、移动菜单、320px/字号/安全区样式、单个统计入口与返回测试 | 手机安全区、系统返回、软键盘真机 |
| A03–A04 | 明确状态/不可用反馈，模式作用域守卫及确认链复用 | 系统 VPN 授权、接管、恢复、模式真实移动提交 |
| A05–A07 | 20 组浏览、类型区分、局部测速、draining、失败项重试与失效守卫测试 | 原生服务持有任务、进程恢复与真实协议测速 |
| A08–A14 | 两分段、编辑草稿保护、系统文档 IO 接线、固定删除替代/停止预览、设置迁移白名单/校验/CAS；规则/DNS/工作区领域复用 | 当前移动配置删除需服务状态，系统选择/保存/分享设备验收、完整工作区及全部表单手机验收 |
| A15–A17 | 端点运行覆盖清理、可信来源恢复链与待应用/未知回执；统计平面、epoch/u64、现有固定目标清空链 | 原生端点桥、来源恢复真实运行验收与独立外部 writer 的 config/generation 原子前置条件、移动 Peer/Host 后端 |
| A18–A19 | 真实 Packet 控制链、完成及实际IPC活动筛选导出、捕获预算/清理/元数据恢复账本、固定脱敏快照及原生回执 | 自由日志安全分类、中断会话最终事实及平台验收 |
| A20–A22 | 固定插件入口与不可用原因，宿主底栏不交给插件 | 移动 VM/SDK IO/来源授权、敏感加载、安装更新卸载与真机隔离 |
| A23–A24 | 两独立真实发行目录/渠道/说明入口、绑定策略与缺失产物区别、实际应用元资料、主动外链/复制、生成构建依赖目录与可读材料/缺口 | 内核随包产物、移动重启/安装、缺失许可资料、iOS 主应用目录与外链手机验收 |

没有合并、发布、改分支保护、购买账号或创建签名凭据。

## 2026-10-14 独立客户端协调检查

本轮新增实际 IPC 活动连接导出、捕获恢复账本、原生前台订阅到期协调、共享 Rust 控制桥与只读检查、共享监听取消/预算/缺口及未知事件脱敏。依赖以根 [todo.md](../../todo.md) 增量维护；core 只读且远端 develop 仍为 e74561f。没有把 C FFI 作为 Rust 嵌入生命周期前置条件。

云端本地 workspace/all-features 测试 **1045 passed / 0 failed / 7 ignored**，fmt、workspace/all-targets/all-features check、clippy 通过（编译警告保留，含未在桌面装配的原生调度路径）。前端完整验证及生产 build 通过，svelte-check **0 errors / 0 warnings**，原移动语义套件76项另加只读桥回执3项；依赖许可17项。完整 Chromium 回归 **211 passed**；系统 WebKit 仍以同提交 CI 实际执行结果为准。

新增真实 Unix IPC 测试验证查询身份前后变化、关闭单监听不终止 pending control/另一 observer、帧/事件/请求预算与秘密脱敏。独立 HTTP/SQLite 测试验证前台同步、修改/手动同步竞争、恢复未知屏障和304拒绝保留材料。这些夹具不等于生产 Zero/手机内核、原生生命周期或真实分享验收。

该次检查时，移动 Proxy/服务注册及随包可运行内核、TUN/保护、后台 OS 调度、304条件缓存语义、iOS主应用/provider与真机转发仍未完成。不创建签名凭据或购买账号。后续条件同步实现见下节；最终 head CI/原生构建收据继续在 PR 描述更新，不借前一提交成功替代。

## 普通订阅条件同步与独立 Proxy owner

普通订阅现在复用共享 HTTP executor，持久 ETag/Last-Modified 与实际来源和有效材料绑定。
真实 HTTP200→304 测试确认 304 不替换材料、不提交运行配置、不修改选择或覆盖；暂停自动
同步仍允许用户手动同步。损坏条件记录有固定来源及版本的显式清理确认，SQLite CAS 仅删除
该来源的条件元数据，不下载、不应用、不解除运行提交结果未知屏障。额度缺失、旧数字 DTO
超出安全整数及来源未提供额度采样时间分别说明；没有合成零值或健康状态。

具体 [RuntimeOwner](../../src-tauri/crates/mobile-runtime/src/owner.rs) 使用固定 core e74561f
的公开 Proxy/RunningProxy，位于独立 workspace，未链接进主应用或 APK。专用 OS 线程及
Tokio executor 持有完整生命周期；真实 Linux TCP 回环在原 UI executor/观察句柄销毁后
继续转发，停止等待实际 shutdown 和 executor disposal。默认及 all-features 各 **5 项通过**，
fmt、check 和严格 clippy `-D warnings` 通过。这不证明 UDP、DNS、所有协议或手机转发。
core 所需八个 vendor patch 使用同一不可变版本，未替换主桌面 workspace 的 TLS 依赖。

云端实际 Android API24 NDK 完整 owner check 在固定 core 的 zero-tun 工厂报 **E0282**：
非桌面分支 `io::Result<impl TunDevice>` 只有 Err，无法推断 concrete 返回类型。
已记录 [CORE-05](../../todo.md)，没有修改 core 或缓存源码，没有用 EngineHandle 或假 Linux
cfg 绕过。iOS owner SDK 检查未运行。PR 的完整 owner iOS job 及 Android owner step 仅保留
手动审计入口，正常 PR 中跳过，不计为目标编译成功。已有轻量共享 Rust、原生文档库和普通
Android 主宿主/APK 检查继续执行；主应用没有实际 owner/TUN 接入。

用户已确认原生 VPN、内核接入及宿主生命周期装配后置；这只改变当前实施顺序。
后台 OS 调度、普通订阅结果未知的持久权威核对/解除入口、随包可运行内核、原生事件桥、
iOS 主应用/provider/App Group、插件执行边界及手机验收仍未完成。

本轮最终云端 Rust workspace/all-features **1057 passed / 0 failed / 7 ignored**，fmt、
all-targets/all-features check 与 clippy 通过，现有编译警告保留。独立 owner 的五项测试与
主 workspace 分开计数。证据为 `/tmp/mobile-round4-final3-{check,tests,clippy}.log`、
`/tmp/embedded-mobile-runtime-{tests,allfeatures-tests,clippy,android-check}.log`。
最终 `pnpm verify:frontend` 与生产 build 通过；移动语义 **84 项**、许可断言 **17 项**，
svelte-check **0 errors / 0 warnings**。完整 Chromium 回归 **214 passed**，包括实际订阅
组件的未知额度、暂停后的手动同步及固定来源条件记录清理确认。证据为
`/tmp/mobile-round4-final3-frontend.log`、`/tmp/mobile-round4-final-browser.log`。
本地 WebKit 未运行，同一提交 CI 双引擎结果和新原生产物收据在 PR 更新，不借用旧提交。

提交前只读复核客户端 develop 仍为 fd198ba5；core 远端 develop 已更新到 c2a72f6，
增量未解除移动接口及 CORE-05，客户端与独立 owner 依赖仍固定 e74561f。只 fetch/read
core 元资料，审计工作树仍干净，无 source/缓存修改。

### Windows 已发布内核回归

`94cfbc7` 的双引擎浏览器 **428 passed**，原生三个工作流均成功；主 CI 的 Windows2025
已发布内核测试有一次真实失败：占用端口场景在仅等待 IPC health 持续300ms 后提前返回。
Windows2022 同场景通过，不能拿对照成功掩盖失败。源码核对确认 IPC/Engine health 不是
listener orchestration 回执，生产 managed host 已使用空 bootstrap，随后等待
`config.apply_runtime` 确认才标记 Running。

真实回归改为上述完整生产顺序；正常场景要求 runtime-only 确认、实际监听及 parent pipe EOF
后资源释放，占用端口场景仍要求实际运行应用被拒绝，没有增加睡眠来制造就绪，没有删测试。
云端从正式 v0.0.1 获取 Linux archive 并验证官方 SHA-256
`1ab96e271219b7c5f03310260c6b823c78ee20e776e33af79d5fe0a6253fce82`，两项真实测试通过；
常规 readiness 五项通过。日志为 `/tmp/mobile-final-readiness-{regression,real-linux}.log`。
Windows 两环境及最终提交的所有工作流结果继续在 PR 描述核对，不借旧提交的浏览器或 APK。

### 插件宿主边界与订阅只读核对增量（2026-10-14）

同一分支和 draft PR 继续交付客户端功能；没有修改 core 源码/缓存、签名、发布或原生 VPN 装配。
客户端 develop 只读复核仍为 fd198ba5，core develop 仍为 c2a72f6，固定依赖仍为 e74561f。

- 真实 QuickJS 执行移到共享有界 worker：2 在途、32 排队、5秒排队期限，原执行/SDK预算与授权TTL不延长；同一原租约跨执行、 owning 提交与交付。停止、撤权、stop/start ABA和宿主关闭拒绝旧启动/结果，在途IO真实 draining。
- 签名页面材料、HTML摘要、组件registration/revision和宿主文档token原子绑定；8个RPC只在桌面注册。固定窗口/channel/代次、前后台关闭/重建、文件选择前后核对和原租约SDK执行已实现。要求Process隔离仍拒绝；VM文件路径不能冒充用户选择，可信票据接入前明确 unsupported。
- 宿主先写完整受控head/CSP/bootstrap，任何guest字节在其后；URL frame导航由parent CSP拒绝。实际本地HTTP探针覆盖pre-head脚本、pre-head图片和注释fake-head，均收到0请求并确认CSP阻止及正常SDK可用。畸形同来源请求也消耗512次预算。
- 普通订阅按固定来源版本/材料/当前IPC身份只读核对，保留无损u64；旧记录没有候选/持久回执时始终result-unknown、canResolve=false。不清屏障、重放、下载、配置写入或创造运行确认。

最新源码云端 Rust workspace/all-features **1089 passed / 0 failed / 7 ignored**，fmt、all-targets/all-features check和clippy通过，编译警告保留。
日志 `/tmp/mobile-phase5-final3-{fmt,check,tests,clippy}.log`。真实已发布Linux v0.0.1内核的两个启动/应用/释放回归通过，
日志 `/tmp/mobile-phase5-final-real-linux.log`；这是已发布桌面内核，不是当前固定core或手机转发验收。

最终前端完整验证/生产build通过，移动语义 **107项**、许可 **17项**，类型检查 **0 errors / 0 warnings**，
日志 `/tmp/mobile-phase5-final2-frontend.log`。真实签名包Host页面/租约/材料回归8项通过；Host执行/ABA/取消及VM文件准入5项由workspace全集覆盖。
实际Frame相关13项Chromium回归通过（12项通道/CSP场景加既有管理流程），日志 `/tmp/plugin-page-csp-fix-final.log`；最终完整Chromium回归另行记录。

本地WebKit缺少执行文件，未运行；最终commit双引擎和原生产物证据以同一PR描述记录，不借旧head。
真实系统文件提供者、OS浏览器、原生WebView、第三方HTTP、移动SDK/敏感加载、进程/OOM隔离和手机VPN均未验收；
原生服务/扩展注册及CORE-01/02/05未解除。普通订阅未知结果的持久权威解除和系统后台调度仍待实施。
详见[执行边界](../gui/mobile-plugin-execution-boundary-v0.0.3.md)、[页面通道](../gui/plugin-page-channel-v0.0.3.md)、[只读核对](../gui/ordinary-subscription-reconciliation-v0.0.3.md)及根[todo.md](../../todo.md)。

上述最终源码完整 Chromium 回归 **228 passed**，日志 `/tmp/mobile-phase5-final-full-browser.log`；没有跳过或重试通过项。

类型化错误收尾补丁随后补齐：原租约的预算、deadline、撤权、过期、关闭及繁忙不再经过通用包/网络错误转换器；直接保留现有SDK canonical code，并用固定文案排除任意原消息/details。实际资源预算、租约超时和撤权/关闭分类使真实Host页面聚焦回归增至11项，Host执行/文件准入为6项。
补丁源码 Rust 全集 **1093 passed / 0 failed / 7 ignored**，fmt/check/clippy通过；最终前端完整验证/build通过，移动语义 **108项**，许可 **17项**，类型检查 **0/0**。
日志 `/tmp/mobile-phase5-typed-final-{fmt,check,tests,clippy,frontend}.log`。同一补丁的完整Chromium运行记录在 `/tmp/mobile-phase5-typed-final-browser.log`；最终终态、双引擎CI和unsigned APK收据以最新head的PR记录为准，634dd29的工作流/产物只算历史证据。


### 4293774 双引擎首次加载回归

上述类型化错误补丁的系统 Chromium 全集实际通过 **228项**；同一提交 GitHub Actions
使用的 Chromium/WebKit 则 **433 passed / 23 failed**，插件 iframe 首次内容没有呈现。
主 CI 其他13项全部通过，不能把本地不同版本浏览器或其他检查通过称为双引擎验收。
移动 Rust 与原生文档工作流通过；unsigned APK 49,552,095 bytes / arm64 / API24，
文档 payload 与许可 asset 检查通过，artifact 11872238395。此提交产物仍没有装配 Proxy/TUN。

精确 Playwright 浏览器下载及失败 trace artifact 本地获取受到环境 CDN/domain 403 拒绝，
不是自动批准审查拒绝，没有修改依赖或绕过网络限制。首次加载与安全握手修复后的新提交
必须重新跑实际双引擎 CI；此处保留失败历史，不能借用上一个 head 的通过记录。


首次加载修复改由当前WindowProxy与新channel的真实bootstrap建立，ready/load/host ack
完成后才接受业务请求；早到空白页或旧iframe回调不建立通道。初始化后的load或pagehide
继续关闭原文档，旧授权、导航和网络CSP断言没有放宽。新增三个实际Frame回归覆盖早到
空白页、多次/延后的宿主load及握手前rawguest业务调用（拒绝且计入预算）。
本地聚焦 **16/16 Chromium** 通过，页面通道Node **8/8**，前端全集验证及生产build通过，
移动语义108项/许可17项，类型检查0/0。日志 `/tmp/plugin-page-handshake-final.log`、
`/tmp/plugin-page-handshake-final-node.log`、`/tmp/mobile-phase5-handshake-final-frontend.log`。
这是系统Chromium154的实际结果；尚不能证明CI Chromium151/WebKit2336首次加载已修复。
Rust及原生配置没有变化，前述1093项验证覆盖同一Rust源码。新head完整CI需要重新核对。


5b95adf 将真实握手与三个顺序回归推到原draft PR 后，标准核对确认初始parent load和
postMessage处于独立任务源，不能用ack往返证明所有load已到达。其CI仍需观察，但不论
终态都继续消除这个歧义：后续load需要原文档对新的host challenge做核对；核对中暂停
调用/交付，pagehide或无法核对关闭原通道。标准依据与最终行为见页面通道文档。


中间提交 **5b95adf** 的实际 CI 双引擎 UI 为 **462 passed / 0 failed**，运行
38518311905 / job115702025361，证明保留的parent/child CSP在CI Chromium/WebKit可执行，
无需放宽URL来源。该通过仍不替代随后消除任务源排序假设的最终源码验证。

最终同文档核对使用每文档bootstrap私有随机proof和每次新host nonce；原window/channel/
epoch同时核对，核对中原请求保留在已有八个在途名额内，不新建业务队列/ID或重放。
返回内容等待前单项1MiB与并发持有4MiB限制；恢复后核对原文档授权。已确认的configure
收据沿其已有owning路径消费同一host快照后关闭旧页，不用变化后的review再授权。
所有匹配原窗口/channel的业务与控制消息共同计入512次/256KiB输入预算。

冻结源码的实际Frame聚焦回归 **22/22 Chromium**、通道Node **8/8**、类型检查0/0，
日志 `/tmp/plugin-page-document-proof-frozen{,-types,-node}.log`。覆盖晚到初load、错误/无回应、
原授权撤销、固定请求等待和旧结果拒绝、自身失效配置收据、捕获监听顺序与控制消息预算。
真实销毁可令pagehide消息source为null；不放宽原窗口校验，实际替换走五秒有界核对关闭，
明确报告文档未确认。当前有效来源的pagehide则立即关闭；不宣称所有浏览器销毁都立即可验。
第一轮夹具的evaluate传参/消息阻断错误和五秒断言竞争已修正，失败不计为通过。
原backend/Rust/原生配置没有改变，1093项Rust证据仍针对同一源码；最新完整前端验证、
最终head的全量双引擎CI及unsigned APK收据在原draft PR继续核对。

最终同文档核对源码 `pnpm verify:frontend` 全集及生产build已通过：移动语义108项、许可17项、类型检查0/0；日志 `/tmp/mobile-phase5-document-proof-final-frontend.log`。本轮未再次运行未改变的Rust全集，前述1093项源码证据与最终head CI各自说明。


79a754a 实际CI为 **473 passed / 1 failed**：唯一失败是WebKit拒绝外部URL后保留原srcdoc，
旧测试误要求其必须关闭。13项其他主CI检查与移动Rust/原生文档/APK工作流均成功；
artifact11874112866为该head的49,551,687-byte unsigned/API24/arm64宿主，不能借作后续head。
测试需要验证实际CSP拒绝/0请求与真正文档替换的旧权限/结果失效，不能把拒绝尝试误作已导航。

随后补齐入站私有文档证明与出站document-bound MessagePort。仅检查WindowProxy及一次
proof不能保证异步回执不被后续文档接收；私有port在bootstrap/guest边界前建立并固定捕获，
源窗口校验仍保持。此增量的完整验证和最终head CI继续在原draft PR核对。


冻结后的child-created私有port源码：实际Frame/既有签名流程 **23/23 Chromium**、通道
Node **9/9**、类型检查0/0、差异检查通过。真正文档替换回归保持相同WindowProxy，
延迟parent load并暂扣旧closing，确认新doc用旧已知channel/错误proof不能调用，旧native
收据在parent load之前也不能被新Window接收；之后有界关闭且不重放。
公共SDK v1调用和Call/Reply DTO未变，port/proof不经公开context提供。
日志 `/tmp/plugin-page-private-port-{final-browser,final-types,node}.log`。
最终 `pnpm verify:frontend` 全集和生产build通过：移动语义109项（包括新增分离入站身份/
出站目标回归）、许可17项、类型0/0；日志 `/tmp/mobile-phase5-private-port-final-frontend.log`。
23项回归的首轮HTTP server收尾因闲置连接挂起，已显式清理连接；CSP/0HTTP/真实替换
断言均保留。新的准确head全部CI和APK收据继续在原draft PR记录，不借79a754a成功产物。

## 恢复执行时的已确认基线

`20603815828c3adafb9e2f5ba2a45a82891f062a` 的四组工作流均成功：
主 CI `38524928261` 的14项检查通过，其中实际 Chromium/WebKit UI job
`115722938791` 为 **476 passed**。移动 Rust `38524928250` 为4项成功、
unsigned iOS Proxy owner 检查1项跳过；Android Rust host中完整 Proxy owner 的
NDK步骤也明确跳过。原生文档 `38524928249` 的3项检查通过。

Android host `38524928259` 成功，匹配同一提交的产物 `11875627777`
（`android-unsigned-host-evidence`）包含未签名 arm64-v8a APK：
49,551,399字节，应用标识 `org.zerodenet.znetsink`，minSdk24、targetSdk36，
原生文档库与payload检查通过。18,641,166字节是artifact ZIP大小，不是APK大小。
这仍是host证据：没有装配实际移动 Proxy owner/随包可运行内核，不证明Galaxy S8
或iPhone VPN、TCP/UDP/DNS、撤权、网络切换、锁屏恢复通过。

后续普通订阅持久回执及本地发布恢复属于新的改动，需独立运行验证并核对新的head CI。

## 普通订阅 confirmed receipt 本地恢复：客户端验证

本批增加固定来源版本及恢复记录的明确本地保存操作。首次读取没有写入；只有持久
权威回执及全部作用域核对通过才提供保存。缺回执、旧未知记录和失效身份保留阻塞。

前端完整 `pnpm verify:frontend` 和生产build通过：移动语义 **115项**，依赖许可
**17项**，类型检查 **0 errors / 0 warnings**，日志
`/tmp/mobile-phase6-recovery-frontend.log`。生产订阅服务Node契约 **22/22**；实际
订阅组件 Chromium154 **11/11**（含旧7项、新恢复4项），日志
`/tmp/mobile-phase6-recovery-browser.log`。覆盖固定source/recoveryId、pending期间
重复/退出阻止、失败保留证据且不重放、prepared及stale保持只读。

这些浏览器断言使用明确的服务fixture验证UI，不充当真实内核应用或真机网络验收。
本地标准Playwright浏览器/WebKit仍未运行；新提交双引擎结果以同一head CI为准。

最终客户端 Rust `cargo test --workspace --all-features --locked --no-fail-fast`
**1116 passed / 0 failed / 7 ignored**（45个结果组）；fmt、workspace
all-targets/all-features check 与同范围 clippy 均通过，Rust/Clippy警告保留。
日志 `/tmp/mobile-phase6-recovery-{fmt,check,tests,clippy}.log`。其中新增SQLite
账本13项、业务恢复9项、真实Receipt结构指纹校验1项；现有实际Unix socket核对
继续证明仅发typed runtime查询，无apply、HTTP或TUN操作。

覆盖schema1/2→3迁移保留关系及能力存储、准备屏障/确认落盘/关系发布/完成阶段、
容量和32项保留预算、SQLite触发器故障整批回滚、损坏候选和摘要、同毫秒编辑、
规则集组合漂移、不同实例/revision、应用设置投影失败、重开及当前进程已知阶段
缓存协调、原版本/原ID重复完成，以及Complete已经提交但结果交付失败时禁止旧
内存回写。首次全集失败源于旧schema1迁移测试仍断言2；已更新为3并检查恢复表为空。
最终全集以上述1116项结果为准，不用早期22项聚焦结果替代。

本批仅在不需外部桌面系统代理重定位的普通活动配置变更中记录可恢复事务；其他
同步仍保留原实际重定位/协调链路。未恢复HTTP validator、没有持久确认回执及
移动owner未注册时的运行核对/恢复均不宣称完成。独立原生Proxy、真机VPN及发行
验收继续遵循现有todo；本批没有修改core来源或缓存。


## 客户端草稿、任务作用域与安全导出增量

本批在同一个 draft PR 继续完成三类可独立验证的客户端工作：

- DNS、VPN/TUN、运行网络和测速参数设置保留未保存草稿，离开需继续编辑或明确放弃；
  提交期间阻止切换，失败保留输入，刷新不覆盖脏表单。
- DNS/路由工具任务将历史与当前结果分开；共享客户端增加 `scopeGeneration`，对配置或
  实例的 A→B→A 变化仍使旧资格失效。普通观测 revision 不会清掉当前结果。开始/停止
  携带原作用域及客户端 revision，由 Rust 在分配任务 ID 或取消前核对。前后台和事件
  恢复重读实际宿主记录，旧完成记录不会重放 Fake-IP 副作用。此处不替代 Zero 的远端 CAS。
- 日志复制、当前筛选导出以及桌面旧诊断入口均使用有界安全投影。筛选固定实际 ID/时间，
  搜索正文不进入请求或导出；未经分类的原文/字段明确省略。预览计划绑定用途、作用域和
  generation，单次消费，交付前后核对；系统取消与结果未知保留实际事实。旧桌面接口
  没有 planId 时拒绝，错误恢复按钮先进入关于页预览，不直接写文件。

实际本地完整 `pnpm verify:frontend`（含生产 build）通过，类型检查 **0 errors / 0 warnings**。
Rust fmt、workspace all-targets/all-features check、全特性 tests 和 Clippy 通过；测试为
**1150 passed / 0 failed / 7 ignored**。编译及 Clippy 既有警告保留。日志
`/tmp/mobile-client-followup-{frontend-final,fmt,rust-check,rust-tests,rust-clippy}.log`。

新增/相关语义验证：任务作用域23项、原任务归属13项及实际 Tauri wrapper/观察器3项；
设置草稿4项、桌面诊断2项、复制安全投影4项。实际组件 Chromium 回归为设置39项、
日志导出4项、桌面诊断8项、工具作用域3项，全部通过；采用明确服务 fixture，不充当
真实内核应用或手机转发证据。系统 Chromium 与 CI 使用的浏览器版本分别记录。

最终推送提交的完整双引擎及四组工作流需另外核对，结果记在同一 PR。
上个 `1433527` 的 CI/产物仅为历史。Android API24 与 Galaxy S8 契约、iOS 无签名/
真实设备限制不变；四项依赖许可缺口仍明确保留。台账另列三项独立客户端后续工作：
端点统计定位与来源返回、Packet 分页、工作区离线草稿与来源冲突/托管只读准入。
它们不归到原生 VPN 后置范围，也不宣称本 PR 完成完整 PRD。


最终复核另复现超安全整数任务 ID 可被回送停止的边界；补丁拒绝超范围任务/作用域身份，
显示明确未确认/不可表示状态，不保存舍入后的控制目标。任务语义增加到26项，覆盖恢复、
事件、启动回执及两类数字作用域与越界u64。Rust源码未变化，前述1150项仍覆盖同一后端；
补丁前4761d57的在途CI仅作历史，新head另行核对。

安全整数补丁冻结后，任务语义 **26/26**、实际组件 Chromium **4/4**、完整
`pnpm verify:frontend` 和生产 build 再次通过，类型 **0 errors / 0 warnings**；
独立只读复核及独立重跑26项也通过。日志
`/tmp/mobile-tool-id-boundary-{node,browser,frontend}.log`。首次容量fixture误用宿主不会
分配的0号ID；改为合法1号后仍验证128条及活动优先，不放宽生产准入。


### 双引擎 CI 时间预算

`9c86efb` 的主 CI 双引擎 job 在20分钟预算处被取消：实际546项、单worker，
已执行Chromium273与WebKit229（共502），44项未完成，日志无断言失败或重试，
不能写成全量通过。job116249712447于04:37:19Z启动、04:57:38Z结束。
其他已完成工作流与50,363,575字节unsigned APK只作该head历史证据。
后续仅将该job预算增至30分钟，仍执行全部用例、原引擎及单worker，不改断言或运行代码。
最终新head的完整结果与产物另在同一PR核对。

## CLIENT-05/06/07：统计来源、真实分页与本地工作区

本批继续同一个Draft PR #46，develop仍为`fd198ba52711f018334860b8a6704546ccd349fd`，
不修改core源码或本批依赖锁，不接入后置的原生运行owner。

- CLIENT-05：实际registry定位共用统计页；固定来源及客户端代次，返回和端点来源的
  reset均重新核对。对象替换、删除、ABA及晚回执不能恢复旧详情；无真实scope不兜底。
- CLIENT-06：真实offset/limit/next_offset，500/500/1分页与末项、当前页刷新及精确重试；
  本页筛选和实时读取有明确说明。原物理目标的进程内关闭屏障跨页/重挂载保留，坏回执
  和未知错误不释放，不重复发送。进程重启持久恢复仍未实施。
- CLIENT-07：真实原生schema/引用校验、Rust JSON文本无损u64、独立SQLite本地保存、
  来源/关系图/AppConfig/规则及客户端代次CAS；托管/受保护来源不返回普通正文。
  脏草稿不被刷新覆盖；唯一导航guard在提交中阻止返回/切tab，完成不自动离开。
- 运行提交固定候选、完整捕获参数及原连接/identity，保持TUN命令15秒响应预算和权限、
  DNS及Windows准备。快速路径在配置提交前后核对真实TUN；未确认原运行材料不自动
  apply本地旧配置或重建旧TUN。纯本地捕获漂移明确拒绝；late drift及缺回执保留屏障。
  实际运行回执与本地发布分阶段，不用历史相同digest确认新事务或盲目整表恢复。

最终本地`pnpm verify:frontend`及生产build通过，类型检查**0 errors / 0 warnings**。
Rust fmt、workspace locked all-targets/all-features check、全特性tests及Clippy通过；
实际**1181 passed / 0 failed / 7 ignored**，45组结果，编译/Clippy非阻塞警告保留。
忽略项仍包括需外部Zero二进制/真实双版本capture/实时订阅的测试及文档测试；不写成已运行。
日志：`/tmp/client050607-frontend-guard-final.log`、`/tmp/client050607-final-rust-{fmt,check,tests,clippy}.log`。

针对性Node：端点与统计59项、Packet18项、工作区及激活24项。
实际Chromium：端点统计17项、Packet320px8项、工作区320px6项均通过；整批先合跑29项，
唯一导航guard修复后工作区6项再跑。分别使用明确fixture，不作为真实手机转发证据。
Packet Rust18项、工作区真实Unix IPC九模式、延迟TUN回执及三项发布代次并发保护包含
在上述最终Rust全集内。完整Chromium/WebKit及四组工作流以本批最终head的PR证据为准，
不复用此前a9744a0的成功、APK或SDK记录。

仍未完成：工作区冻结材料重启、安全启动通道及watchdog/CAS，工作区原操作和Packet关闭
屏障的进程持久恢复，远端原子前置条件、原生VPN/core装配、后台调度/插件/真实安装与
Galaxy S8/iPhone验收。A11及完整PRD不因本批通过而完成；四项许可材料限制继续保留。
契约：[工作区](../gui/config-workspace-local-v0.0.3.md)、[统计导航](../gui/endpoint-statistics-navigation-v0.0.3.md)、[Packet分页](../gui/packet-route-pagination-v0.0.3.md)。

### c422292 完整CI失败及修复

c422292的主CI真实执行608项：Chromium303通过/1失败，WebKit302通过/2失败，
总605通过/3失败；三项各重试一次仍失败，没有flaky或skip，非超时。其余13个主CI
job及另外三组工作流成功，仅作为该head历史证据，不能称本批最终全量通过。

修复共享settings fixture误用专项u64样例：保留原日志、客户端覆盖和最终inbounds三视图，
专项样例只供workspace panel。移动返回只把尚未关闭的dialog视为顶层；真实Portal已
closed但仍在退出动画时，不吞掉下一次浏览器/系统返回。原有返回详情断言保持。
新增两项显式保留2秒退出动画的回归；暂用旧逻辑时，两项都到达原详情返回断言并失败，
恢复修复逻辑后通过。最初仅改animation-duration的试验未保留Portal，停在前置条件，
不计为正反验证结果；使用显式keyframes后的日志才是有效证据。

修复后相关Chromium **75/75通过**，涵盖端点/共用统计、移动shell、旧端点/统计、
settings三视图和工作区；完整pnpm verify:frontend、生产build、类型0/0通过。
Rust源码及依赖未变，前述1181/0/7仍对应同一后端；新head CI另行核对。
本地WebKit官方下载返回403 Domain forbidden，未运行、未绕过；以同提交CI实际执行为准。
日志：/tmp/client050607-ci-fix-browser-verified.log、/tmp/client050607-ci-fix-frontend-final.log、
/tmp/client050607-closing-portal-negative-final.log。最终准确head、双引擎及新产物见同一PR。

## 原操作与 Packet 持久恢复（a51c6ce 后续）

本批在同一 Draft PR 增加普通工作区 schema4 原操作账本、持久关系 epoch 和随机
宿主会话。副作用前持久预留，只有原真实回执持久确认后才可明确检查并完成本地
原候选；SQLite 关系、AppConfig 确认和完成墓碑分阶段，不重放 apply/TUN/下载。
预览后的关系 A→B→A、宿主重建后的相同本地计数、旧票据及晚回执均不能取得旧执行
资格。AppConfig/规则文件仍只核当前值，不声称具有全部历史 epoch。

Packet 账本使用私有有界元资料、跨进程文件锁、原子持久预留和原 operation token。
读取列表缺行或已结束行不清账；只读核对原物理目标的权威结束事实或明确拒绝后，
还需清账持久确认。坏记录、丢失已初始化账本、写失败和晚 token 保留屏障。

审查中修复了 Complete 后内存屏障未退役、内存投影后可失败收尾漏配置失效通知、
正常 apply 换用新 binding、预检未固定持久 epoch 等边界。真实回执保存为历史事实
不因本地新 epoch 丢失；本地发布仍严格核对原 epoch。系统调时保留原始时间和标记，
一次性恢复票据按进程单调时间到期。冻结材料重启仍未实施，具体源码证据和安全
通道、实际依赖字节、路径、child/readiness/receipt/watchdog 条件已补入 todo。

最终本地 Rust workspace/all-features 测试 **1224 passed / 0 failed / 7 ignored**，
共45组，GUI lib为951/0/5。包括真实OS子进程重开、跨进程锁争用、SQLite回执与阶段
故障、实际文件权限拒绝、关系ABA、并发预留、票据过期及真实Unix IPC纯查询恢复。
专项工作区41/41、Packet37/37均通过；子进程子结果不再重复计入父harness总数。
7忽略项为外部Zero配置/观测、两代快照、两项真实发布内核readiness、live订阅provider
及Lifecycle文档测试；本地没有将它们计为执行。格式和全workspace/all-targets/
all-features check和同范围Clippy均通过；编译及Clippy警告保留，未按`-D warnings`
宣称零警告。准确提交CI以同一PR记录为准。

前端完整 `pnpm verify:frontend`、生产build和类型检查通过（**0 errors / 0 warnings**）。
Node工作区29+profile7=36、Packet26通过；320px实际Chromium旧14+新增9=
**23/23通过，0重试**。浏览器只模拟宿主事实重建，不能替代上述Rust真实进程测试，
也不能作为移动VPN验收。本地WebKit仍未安装，官方下载403没有绕过；待最终SHA CI
执行完整双引擎套件，不以a51c6ce的612例或旧产物代替。

早期专项实际出现18/8、34/5：测试候选错误省略真实合成、初始AppConfig未经过
生产规范化，以及重开测试遗漏私有目录；修复测试材料和路径，未放松校验。全量
首轮另有一个schema迁移断言仍期待3而实际为4；更新该版本断言并保留旧元资料检查后，
全集重跑通过。浏览器编辑期间的一次HMR重建失败在冻结所有相关源码后23例重跑通过，
未删除原分页重试断言或将其计为flaky通过。

日志：`/tmp/client-persist-workspace-test-third.log`、
`/tmp/client-persist-packet-tests-initial.log`、`/tmp/client-persist-final-tests-fixed.log`、
`/tmp/client-persist-final-{fmt,check,clippy}.log`、
`/tmp/client-persist-frontend-initial.log`、`/tmp/recovery-browser-freeze.log`。
本批没有新增依赖，四项许可材料缺口、原生装配、Galaxy S8/iPhone及签名/安装未验收项保留。

### 2eceeb4 的 Windows 订阅握手失败

该提交的 Windows latest 已实际通过客户端 **1154/0/5**，随后发布版内核 readiness
为 **1 passed / 1 failed**：IPC 健康检查完成后，新控制连接等待 subscribe ACK
5000ms 超时。Windows2022 同提交客户端与 readiness 合计 **1156/0/5**；对照通过
不能代替失败平台结果。Android unsigned host、Mobile Rust、Native document adapters
三组同提交工作流成功。主 CI 终态为13成功/1失败，双引擎 **630/630** 通过、
0重试/0flaky/0跳过；不能将这些成功部分称为该提交全绿。

只读核对实际发布的 core v0.0.1 `src/ipc/windows.rs`：同一 select 同时等待新管道连接
和旧连接 task 完成，旧 task 先结束可能使当前监听实例被丢弃。这是失败的候选原因，
未在此 Linux 环境声称复现 Windows 调度。客户端另有可确定的边界问题：reader
在 ACK 前退出时，Inner 仍持有 ACK sender，连接方因此等待整个超时才知道已断开。
本轮仅修复首次订阅、尚未交付业务请求的冷握手；已发送的 query/command 不因此
重放，也不重新发现 BoundControl 的目标。实际平台结果须继续核对修复提交的 CI。

readiness 占用端口测试同时加强：订阅握手失败本身必须使测试失败，不能当作实际
运行配置调用失败；应用失败回报原内核 stderr。没有放宽运行确认、API 身份或权限检查，
也未修改 core 源码。

修复只允许 reader 的实际断连信号在 ACK 前触发最多三次冷订阅；三次共用原截止时间，
每段订阅写入及 ACK 等待使用剩余预算，成功后恢复原业务写入预算。内核拒绝、坏 ACK、
解码错误、订阅写失败和 ACK 后断连均不在此重试边界。CONNECT_GATE、缓存发布及
BoundControl 的原物理连接保留。Unix 同步 connect、Windows 管道连接的既有底层等待
仍按 transport 实现，不声称本次增加了全部 OS syscall 的硬总截止。

修复后的实际 Unix IPC 控制连接专项 **14/14**，其中新增六项覆盖即时 EOF、第二次
ACK、最多三次、迟到 ACK 不刷新预算、拒绝/坏 ACK/坏帧和业务零重放。全 workspace
all-features 本地测试 **1230 passed / 0 failed / 7 ignored，45组**，GUI lib
**957/0/5**；忽略项范围与上一批一致。格式及 locked workspace/all-targets/
all-features check、同范围 Clippy 通过，保留现有警告。复核补回手动订阅写循环对
Interrupted 的继续语义后，最终格式/check/全集/Clippy 均重新通过上述结果。
前端源码未改，本地不重复执行刚完成的冻结前端套件；
修复提交仍需重新核对完整 CI，不将 2eceeb4 的 630 项或 APK 充作新 head 的产物。
日志：`/tmp/client-handshake-focused.log`、
`/tmp/client-handshake-final-{fmt,check,tests,clippy}.log`。


## 2026-10-15：文件历史与只读账本诊断

本批从全绿 `d8e6ac1272b02b4064044deb195206ac0e203950` 继续，在同一 Draft PR 中增加
AppConfig/规则正式持久写入的 OS 文件锁、随机存储身份、单调 epoch、原操作 intent/receipt
和主备字节核对；工作区、普通订阅及端点恢复固定原文件 tokens，拒绝合作 writer 的
同值保存与 A→B→A。schema5 保留旧数据；旧无 tokens 记录仅作历史，不补发控制许可。
原 owned 文件写已完成、检查点中断时，纯查看仅 probe，明确原操作恢复才可补确认
元资料；不重复写正文、不 apply、不下载。没有原 intent 或材料变化时保持未知。

工作区与 Packet 增加显式有界只读检查及固定错误分类，原草稿、文件和未知屏障保留。
工作区不读候选列，不进入正常创建/迁移/修复路径；SQLite READ_ONLY 仍可能写 SHM，
因此实际存在 WAL 时直接报告原因，无 WAL 时使用 immutable 只读主库并再次核对变化。
有真实临时库/WAL/SHM 保留与子进程检查测试，未操作用户数据。没有归档/删除/清账按钮。

冻结启动仍 unsupported；已逐项记录普通空 bootstrap、当前材料重读、Proxy 实际启动
顺序及公开快照缺失字段。PID/health 不确认 startup-only 字段；客户端安全材料/依赖封存/
watchdog 可独立继续，实际 Proxy startup acknowledgement 的最小协调范围见
[启动边界](../gui/frozen-startup-boundary-v0.0.3.md)，没有修改 core。

许可补入 svelte-toolbelt 0.10.6 精确包 LICENSE 的显式已审阅 MIT 映射，保留
manifest 未声明的事实及原两作者版权。其它三包仅补真实审计来源/限制；四项原缺口仍
保留，不用 sibling 版权或通用条款冒充该包材料，不声称最终链接材料完整。

本地 locked workspace/all-features 测试 **1292 passed / 0 failed / 7 ignored，45组**，
GUI lib **1019/0/5**。包括 file_history 10、SQLite只读检查15、Packet只读检查13、
端点文件历史10、普通订阅服务15与数据库17项；子进程输出不重复计入父 harness。
忽略项范围与前批一致，未当作执行。首轮两个失败分别是迁移 fixture 仍期待 schema4、
ABA fixture 捕获了规范化前 AppConfig；修正基准并保留旧数据/主备原字节及旧票据拒绝
断言后，完整重跑通过，没有放松生产校验。

完整 `pnpm verify:frontend`、生产 build、类型检查通过（0 errors / 0 warnings）。
新增只读诊断 Node16/16；冻结 Chromium 31/31 后 WAL 分类新增1例，最终该页9/9，
均0重试/0失败/0跳过；浏览器使用宿主事实 fixture，不代替真实进程或手机验收。
本地 WebKit 未运行，官方下载403没有绕过，最终双引擎结果以新提交CI为准。

日志：`/tmp/client-history-tests-{initial,final}.log`、
`/tmp/client-history-frontend-final.log`、`/tmp/storage-inspection-{node,check,browser}-wal.log`。
本批不改版本或 lockfile；只为已有 rusqlite 启用 hooks/limits 来限制只读 SQL 预算。
新 commit CI、无签名 APK 及 SDK 收据在同一 PR 中逐项记录，不借前批全绿替代。

最终源码又经 fmt、locked workspace/all-targets/all-features check、完整 tests 与同范围
Clippy 通过；测试仍为1292/0/7。Clippy 首轮拒绝两处权限测试的十进制零，修为八进制
`0o0`，同时简化两处新代码冗余引用/闭包；随后全集和检查重跑通过。现有及非拒绝级
警告保留，没有按 `-D warnings` 宣称零警告。最终日志：
`/tmp/client-history-tests-after-clippy.log`、`/tmp/client-history-fmt-final.log`、
`/tmp/client-history-check-final.log`、`/tmp/client-history-clippy-fixed.log`。

实际 Rust 许可 build-hook fixture 也通过：首次执行缺少云端 Cargo PATH 未启动检查，
补齐与正式构建相同工具链环境后成功，日志 `/tmp/client-history-license-build-hook-fixed.log`。
