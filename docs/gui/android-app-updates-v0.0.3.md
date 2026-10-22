# Android 应用更新安装契约

本轮基于 PR #46 的 `51f6a5136afef627079b02286c2c92ff8a8565c4` 实施；最终提交、构建与 CI 由同一 PR 的验证记录绑定。
最低 Android API24，target API36。iOS 专项继续暂停，VPN/core 运行装配仍后置。
本文描述代码行为，不能作为系统安装、API24 或 Galaxy S8 的运行通过记录。

## 发行身份与安装准入

客户端目录继续读取固定 `zerodenet/znet-sink` GitHub Releases API，沿用稳定、候选、开发渠道映射。
新增 `androidAssets` 只包含已上传、大小为 1–512 MiB、合法 APK 名称、精确仓库/tag 下载 URL，
并且 GitHub API 返回完整小写 `sha256:` 摘要的资产。后缀仅用于列出候选，不能判定可运行。
内核目录不产生应用安装资产；移动独立内核安装和桌面 updater 均不经此路径开放。

主窗口只传 tag、资产 ID、审阅摘要、渠道和原操作 ID。Rust 重新读取受验证目录并匹配这些身份，
不接受页面提供 URL、文件路径、包身份、签名或任意摘要作为授权。
没有合格资产时保留明确原因和项目资料入口，不生成清单、摘要、版本代码或成功回执。

下载完成后，原生层检查实际大小与 SHA-256，并从真实 APK/PackageManager 读取包名、versionName、
versionCode、minSdk、当前签名证书及 ZIP 中 native library ABI。要求实际包名等于当前安装包，
版本名称等于所选发行，版本代码严格上升，API/ABI 支持当前设备，签名证书集合与已安装包完全相同。
API24–27 用 GET_SIGNATURES，API28+ 用 SigningInfo 的当前 APK signer；暂不自动接受签名轮换。
ZIP 条目与展开大小有预算，不解包到任意目录。真正 APK 签名验证、系统策略和开发者验证最终由 Android 安装器裁决。

`org.zerodenet.znetsink.debug` 与正式包身份不同，不互相更新。此前临时 Debug 签名密钥未保留，
新建 Debug APK 也不能据此宣称可升级旧 APK；本实现不创建或保存发行签名凭据。
Google Play 安装来源使用真实包 ID 的 Play 更新页面，不通过私有 APK 替代商店流程。

## OS 下载与恢复

`NativeAppUpdateDownloadJob` 是独立 JobService，由 JobScheduler 持有，使用持久作业和网络约束。
应用私有 noBackup 容器的 AtomicFile 保存固定操作 ID、目标、摘要、大小、阶段、尝试次数、会话和回执；
主页面退出、Activity 重建和普通进程死亡不依赖 JavaScript 定时器继续下载。
前台版本页仅定期读取进行中操作的 journal 展示进度；隐藏或离开页面停止该观测，不调度、重试或执行安装。

每次从固定 GitHub HTTPS 下载 URL 开始，最多四次 HTTPS 重定向，只允许 GitHub 与其两个发行资产域名；
保持系统 TLS 校验，不传用户 token、请求头或其他来源的身份。请求有连接/读取超时，正文至多十分钟，
每次写入前检查目标身份、取消与声明大小，实际字节超过大小立即失败。
最多八次尝试、总意图有效期四十八小时；断流不拼接未验证片段，后续合法 OS 重试从零开始。
每次开始还检查私有存储余量。成功后先检查 APK，原子转为待安装，绝不下载后自动提交安装。

切后台、锁屏和 Doze 允许系统推迟或停止作业；onStopJob 中断连接，保留可观察的暂停状态并请求有预算重调度。
系统强制停止、用户卸载、OEM 策略、作业配额与无网可能阻止执行，不承诺准点或永久运行。
恢复 UI 只读取原生作业、journal 与安装包；缺少作业/会话、损坏 journal 或失去确认票据显示未知/读取失败，
不重置为成功，也不自动重新下载或重放安装。显式取消后，可重新审阅新操作。

## 系统安装、取消与确认

安装前再次核对私有 APK 及当前安装身份，并记录固定 operation/session/callback token。
使用 PackageInstaller 完整安装会话；API31+ 明确 USER_ACTION_REQUIRED，API24+ 保留系统正常确认要求。
target API35+ 使用显式私有 receiver 的 mutable callback PendingIntent，以接收系统填写的结果；
回调重核精确 session ID 与随机 token，过期、伪造和其他操作回执不改变当前操作。
系统 commit 返回后才记录提交回执。准备期进程中断或缺少提交回执均转为 UNKNOWN，不能将持久的中间阶段当作仍有活跃执行者。

STATUS_PENDING_USER_ACTION 保存系统持有的确认 PendingIntent 和过滤身份，只有用户在可见、有焦点的宿主页面
点击后才打开该系统 Intent；后台 receiver 不启动 Activity。普通进程重建后可重新取得 OS 票据，
重启后 OS 票据已失效时明确显示未知，要求核对或取消，不合成替代安装确认 Intent。

API26+ 先读 canRequestPackageInstalls。没有许可时显示原因；用户可明确打开此应用的系统安装设置。
API24–25 显示系统确认/安全设置判断，不能猜测未知来源已经开启。代码不更改未知来源、安全开关或静默许可。
系统取消/验证终止、策略阻止、包冲突、不兼容、无效包、存储失败分别保留系统结果，系统原始异常消息不进入普通 UI。

下载取消停止 OS 作业并清理此模块所属暂存文件；安装取消尝试放弃原 session。
已经提交而无法确认放弃的操作保持 UNKNOWN，不能保证撤回已完成的系统安装。
下载成功、会话提交、安装器 STATUS_SUCCESS 与安装确认是不同阶段。
应用前台恢复及 MY_PACKAGE_REPLACED receiver 都读取真实已安装包名、准确版本代码/名称和当前签名；
全部与原目标吻合才显示 installed-confirmed。缺回执不自动换 ID 重放，不声称旧版本一定仍有效。

## 验证范围与仍需设备证据

宿主测试覆盖受验证目录资产、无损计数、固定审阅目标、UNKNOWN 禁重放、页面销毁与晚到结果；
JVM 测试覆盖包/版本/API/ABI/证书拒绝、下载来源、真实流的超限/截断/取消与时间预算。
Android instrumentation 代码使用真实 PackageManager 检查自身 APK 的“非更新”拒绝、坏摘要及伪造回调；
还覆盖 AtomicFile 超大备份，以及空会话准备中断/缺少提交回执的 UNKNOWN 恢复。
后者只创建并最终放弃一个测试所属的空安装会话，不写入 APK、不 commit；这些测试不改变系统许可、不创建签名凭据，也不自动安装包。

正向系统安装、用户取消、进程恢复/包替换、API24/Galaxy S8、锁屏和 OEM 限制必须另行在设备执行，
需要实际发布且与已安装包同签名、版本代码上升的 APK。仅编译 test APK/AAR、宿主测试或 Debug 打包不能替代这些验收。
若目录尚无合格签名资产，记录为发行材料缺口，不能伪造可安装发行物。本轮不合并、发布或操作用户 Mac。

2026-10-22 通过 GitHub 连接只读核对最近十个公开发行（2026-09-28 至 2026-10-16）：
每条二十三份资产，均没有 `.apk`。此证据仅覆盖本次查询的十条，不能推出全历史没有 Android 包。
记录位于云端 `artifacts/mobile-v0.0.3/android-background-update/release-assets-observed.json`；
没有正向更新所需的实际 APK 发行材料，不能把目录读取成功当作安装通过。

本模块已执行宿主发行/更新行为测试三十二项、Svelte 类型检查零错误零警告，
Chromium 的 320 px 更新页面 fixture 三项通过（摘要与目标固定、缺回执不重放、无合格资产禁下载）。
这些 fixture 使用显式测试回执，不是 Android 安装结果；原生 Gradle/JVM、仪器测试编译和最终精确提交结果由验证日志分别登记。

依据：[PackageInstaller](https://developer.android.com/reference/android/content/pm/PackageInstaller)、
[SessionParams 用户确认](https://developer.android.com/reference/android/content/pm/PackageInstaller.SessionParams#setRequireUserAction(int))、
[Session.commit](https://developer.android.com/reference/android/content/pm/PackageInstaller.Session#commit(android.content.IntentSender))、
[安装来源许可](https://developer.android.com/reference/android/content/pm/PackageManager#canRequestPackageInstalls())、
[JobService 停止与调度](https://developer.android.com/reference/android/app/job/JobService#onStopJob(android.app.job.JobParameters))。
