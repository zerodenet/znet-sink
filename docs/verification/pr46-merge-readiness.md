# PR #46 合并范围与风险审计

2026-10-20。范围为 `develop` 的客户端开发增量，不是移动产品发行验收。
审计基线 `e752af0429099a68b4d9cd9dfa317de384d8b173`，实际 develop 为
`d39ad2787fb05052fa15065853ab71526441ccc3`，已包含在工作分支。
GitHub 当次读取为 open、draft=false、mergeable=true，最终提交的检查状态以 PR 为准。
本次不切换 Draft/Ready、不合并、不发布、不修改保护规则或权限。

## 合并判断

可按「共享客户端安全与数据完整性增量、移动布局及管理流程、开发宿主基础」收敛，
在最终提交的常规 CI 全部实际通过、无合并冲突、维护者接受下列范围后合并 develop。
本审计不是全部代码逐行安全认证；PR 涉及多个客户端领域及大量许可材料，仍有范围与
回归风险。没有由移除自动移动构建推导出产品完成、真机通过或可发布。

非阻断的已知体验缺口还包括共享 `DraggableModal` 的标题与可访问名称关联；本次不把
隔离功能草稿中的改动一起带入。旧数据升级、磁盘不足、安装失败及真实设备提供者仍须
在后续发行前验证，不能由当前 fixture 或编译通过推断全部迁移安全。

合并前必须修复：最终头的真实测试/类型/契约失败，权限或旧结果准入绕过、数据丢失、
未知操作自动重放、虚假成功、缺失检查或不可解析工作流等问题。本次没有用 skip、
放宽断言、延长超时或绿色占位任务处理这些失败。最终检查仍在执行时不能提前批准。

本轮不加入尚未提交的文件票据 SDK。该工作完整保留在隔离工作树和本地 WIP 分支，
不进入本 PR 的提交、门禁或完成统计；其测试结果也不作为本 PR 的验证证据。
浏览器原生返回等新功能不继续扩展。

## 已交付与仍未完成

| 范围 | 已提交代码及抽审结论 | 尚未完成及影响 |
| --- | --- | --- |
| 导航与管理 | 四底栏、唯一概览统计入口、多组/嵌套节点、配置/订阅、路由/DNS、工作区、端点/统计/连接/Packet、日志/诊断、版本与关于沿用真实客户端链；保留高级入口和 unsupported 原因 | 浏览器组件测试不是手机网络或设备验收；各 A 项不标为整体通过 |
| 数据与副作用 | 固定目标预览、原配置/实例/版本及权限、CAS、原操作回执、未知结果屏障、无损大整数和有界记录；部分失败不当全成功 | 跨配置/受保护材料迁移、授权文件依赖 resolver、泛化资源替换/恢复仍受限，不能绕开只读拒绝 |
| 工作区 | 三视图、本地保存/待应用、原生校验、允许的 runtime-only 路径 | `commands/config_workspace.rs` 对 Restart 明确拒绝，缺冻结启动材料及回执；内核管理普通重启独立存在。不能称完整工作区重启应用已交付 |
| 插件 | 已签名材料、逐组件权限、可信页面通道、前台解释器所有者、安装审阅、固定资源卸载与历史记录 | `services/plugins/platform.rs` 保留 FilesSelection、BrowserCallback、RuntimeProtectedLoad 的移动 unsupported；没有把隔离 WIP 开放。进程/OOM 隔离、OS 后台和完整 A20–A22 未验收 |
| 原生秘密 | `services/plugins/vault.rs` 及原生 vault 检查既有身份/包络，丢失或不匹配密钥拒绝，不降级普通主密钥或伪造重新授权 | iOS 已观察到 Keychain 安全拒绝，诊断不解除生产准入；Android Keystore 模拟器不是完整插件或 Galaxy S8 验收 |
| VPN/内核 | `mobile_host/mod.rs` 分开内核/VPN/观测未知态；默认随应用分发的策略保留；Linux RuntimeOwner 实际持有 Proxy 并有回环转发测试 | 原生 VpnService/Packet Tunnel、完整 Proxy 注册、外部 TUN、protect/bind、网络变化、默认可运行移动内核尚未装配，已后置；不能用 EngineHandle 或 UI 存活代替 |
| 更新与后台 | 有界版本目录、前台到期协调、真实 200/304、持久意图和任务未知屏障；移动端拒绝桌面安装/执行回退 | Android 系统安装器/发行渠道、OS 后台调度、安装后版本确认、活动来源替代/停止、手机资源恢复仍待实现和验收 |

抽审入口还包括 `services/kernel_manager.rs` 的桌面运行准入、插件 callback 原授权绑定、
调度 dispatch/lastAttempt、配置与订阅恢复、统计 epoch/generation、Flow/Packet 固定关闭
及前端 scope 失效。完整细分仍见[完成矩阵](../gui/mobile-client-completion-matrix-v0.0.3.md)
和 [todo](../../todo.md)。已修复的 UI Escape/返回层级不等于浏览器原生授权回调已接入。

## 平台与验证边界

Android 最低 7.0 / API24，已有 arm64 NDK check、Debug/unsigned Release 打包及独立
API24 Keystore 模拟器记录；具体提交分别记账。WebView 111 是显式启动条件，仍须
Galaxy S8 实机验证字号/键盘、内存、文件提供者、前后台/进程重建、权限和网络变化。
当前没有 Galaxy S8 验收，也没有把浏览器 Chromium 当 Android WebView。

iOS 专项暂停，保留源码、16.4 App 契约与既有 SDK/Simulator 证据；不购买会员、创建
签名凭据或声称真实 iPhone VPN 验收。`6fd3dc4` 的真实隔离诊断停止在创建前读取，
OSStatus=-34018，创建/回读未执行，SDK accepted:false；不据此推断必须购买账号。

Android 许可仍有四项原始资料/声明缺口，覆盖 svelte-toolbelt、selectors、tao-macros、
rustls-platform-verifier-android 的各自限制。保留未知与来源，不能拿别包材料补齐。
详见[许可资料复核](../gui/dependency-license-inventory-v0.0.3.md)。这不是完整合规结论。
签名发行、商店、商业化和发布前资料审阅不在本次授权内。

## 常规自动检查与手动移动验证

保留原 push/PR 触发及已有检查名称，在 job 级明确限制手动执行。被跳过的 job 是
**未执行**，不能写成编译、安装、模拟器或设备通过。手动运行继续使用原测试脚本和
原断言；没有把已知真实失败改成成功占位。此前 CORE-05 移动 owner 编译依赖仍保留。

| 工作流/任务 | 常规 push/PR | 手动 workflow_dispatch |
| --- | --- | --- |
| CI：桌面产品组合、共享客户端、前端/类型/构建、Chromium/WebKit及聚合门禁、Windows及下载/签名/回退/许可契约 | 保持原门槛；新增工作流策略检查 | 原入口保留 |
| Mobile Rust：Linux Embedded Proxy ownership/check/test/clippy | 执行普通宿主测试，无移动 SDK | 保留 |
| Mobile Rust：Android/iOS target matrix、Android API24 NDK、iPhoneOS owner SDK | 明确跳过 | 原交叉编译与证据保留，CORE-05 条件不伪装解决 |
| Native documents：JVM纯策略与真实 JCE 包络、Linux Swift Foundation纯策略 | 执行，无 Android SDK/NDK、iOS SDK 或模拟器 | 保留 |
| Native documents：Android库/测试APK、API24 Keystore模拟器、iOS UIKit库 | 明确跳过 | 原编译、仪器测试和产物检查保留 |
| Android Debug App、Android unsigned Release App、iOS完整App simulator/device | 明确跳过 | 原完整脚本、原架构/签名/许可/包和实际UI检查保留 |
| release.yml、builtin-rules.yml | 未修改事件、策略、构建或发布逻辑 | 未修改 |

`CI / UI controls (Chromium and WebKit)` 仍只依赖真实 `ui-browser` 结果，要求 success，
不依赖手动移动 job。没有增加 `continue-on-error` 或“未构建但通过”的聚合步骤。
直接保护规则读取有 403 可见性限制；可读分支元数据当次为 protected:false、contexts[]，
rulesets 列表为空，不能替代完整权限审计。保留触发/名称避免潜在 required 检查缺席，
不绕过受限接口，不改规则；合并时仍以 GitHub 实际 required 状态为准。

### 明确手动入口及默认分支限制

上述各工作流保留 `workflow_dispatch`。GitHub 要求工作流定义存在于默认分支，才可
触发该事件；[官方说明](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)。
本仓库默认是 main，当次 main 只有 ci/release/builtin-rules 三份定义，因此**移动 Run
workflow 按钮当前不保证可用，合并 develop 也不等于已进入 main**。本次不推送 main、
更改默认分支或增加发布操作。维护者将定义按正常流程纳入默认分支后，可选择准确 ref：

```sh
gh workflow run android-debug-host.yml --ref develop
gh workflow run android-unsigned-host.yml --ref develop
gh workflow run native-documents.yml --ref develop
gh workflow run mobile-host.yml --ref develop
```

在具备相应工具链的云端 checkout，也可直接执行既有入口，保存新鲜证据：

```sh
bash scripts/build-android-debug.sh
bash scripts/test-native-io-android.sh
# 仅在已准备的 API24 模拟器上显式执行：
NATIVE_IO_ANDROID_RUNTIME_TESTS=1 bash scripts/test-native-io-android.sh
```

unsigned Release 的目标许可生成、Gradle hook、构建及 unsigned 验证沿用
`.github/workflows/android-unsigned-host.yml` 全部步骤，不只取一条打包命令冒充验收。
保留的 iOS 入口为 `bash scripts/build-ios-host.sh simulator|device`，仅后续明确启用的
云端 macOS/SDK 环境适用，当前专项暂停。本轮没有执行这些重型手动入口或本机模拟器。

## 本次收敛验证

工作流策略 **2/2**；所有 YAML 由本地 BaseLoader 解析；shell 语法检查通过。原策略
JUnit **15/0/0/0**（IO 14、JCE 包络1），直接使用生产纯策略和原测试，包络提取保留精确
源摘要，不编译 AndroidKeyStore/Context，不是 Android Keystore 运行验收。Swift纯策略
继续由普通 Linux CI执行。类型检查 **0错误/0警告**；已有宿主生成/回执 Node 契约
**11通过**；release-policy 回归通过。发布工作流和策略文件与基线逐字节未变。

首轮策略测试因引用不存在的旧 job ID 为1通过/1失败，修正为实际原检查 ID 后2/2；
没有删断言。最终提交 SHA、实际 CI运行及失败/重试历史在 PR 当前说明记录；新头未
结束前，不用 e752 或隔离 WIP 的成功结果代替新头门禁。
