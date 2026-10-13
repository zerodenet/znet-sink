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
| A18–A19 | 真实 Packet 控制链、完成历史筛选导出、捕获时长/字节预算和清理 barrier、固定脱敏快照及原生回执 | 活动记录导出、自由日志安全分类、捕获进程终止状态持久核对及平台验收 |
| A20–A22 | 固定插件入口与不可用原因，宿主底栏不交给插件 | 移动 VM/SDK IO/来源授权、敏感加载、安装更新卸载与真机隔离 |
| A23–A24 | 两独立真实发行目录/渠道/说明入口、绑定策略与缺失产物区别、实际应用元资料、主动外链/复制、生成构建依赖目录与可读材料/缺口 | 内核随包产物、移动重启/安装、缺失许可资料、iOS 主应用目录与外链手机验收 |

没有合并、发布、改分支保护、购买账号或创建签名凭据。
