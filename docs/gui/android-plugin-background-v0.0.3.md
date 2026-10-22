# Android 插件系统后台任务

本轮在 PR #46、基线 `51f6a5136afef627079b02286c2c92ff8a8565c4` 后补齐 Android API24 的实际 OS 调度及无界面执行入口。iOS 专项暂停；VPN、TUN、内核运行装配仍后置。本文是代码契约，设备运行结果另列，不代表 Galaxy S8 验收。

## 系统持有执行

`NativePluginJobService` 由 Android JobScheduler 唤醒，同一应用进程，无 Activity、WebView 或网页定时器依赖。服务只接受系统的 `BIND_JOB_SERVICE` 调度。一个持久周期作业合并检查已有插件任务，不另建业务队列。`ACCESS_NETWORK_STATE` 是 target API34+ 使用网络约束作业所必需的普通权限，不触发运行时授权或修改系统开关；`RECEIVE_BOOT_COMPLETED` 仅满足系统持久调度要求，不申请精确闹钟、常驻前台服务或电池豁免。

已有任务意图仍为 5 分钟至 7 天。后台最小系统周期为 15 分钟，需要可用网络，Doze、锁屏、省电、配额、OEM 和网络约束可进一步推迟；不承诺准点。系统强行停止应用后，需用户重新打开，再核对登记与记录。原生 `ensure` 不重复改写已匹配的系统作业，避免每次恢复前台都把周期窗口向后推。系统登记失败与作业标识冲突明确显示，不能只因代码编译就宣称已登记。

单次系统派发窗口 90 秒，Rust 接纳预算 85 秒（停止后在途 IO 可能继续排空），最多固定 8 个到期任务。队列以原 `revision`、安装 generation、实际权限、材料身份、原生宿主生命周期与 attempt ID 接纳。任务执行的是真实受限解释器与 SDK；没有模拟采样或替代成功结果。前台仍复用原队列检查，任一 owner 的既有 flight/dispatch 记录阻止重复发起。

## 冷启动与共享服务

JNI 冷入口从真实 Android Context 读取包名、私有数据目录和设备形态，生产包与 Debug 包分开校验。它不接收页面传入的路径、目标或权限。Rust 初始化同一份 `AppState` 服务 owner；界面稍后创建时共享其 Arc、授权管理器、任务 flights、配置及 SQLite 缓存，不复制第二份状态。并发初始化对同一规范化容器幂等，不同容器拒绝；损坏配置不能作为空缓存发布。

后台许可仅授予本次 OS 作业的执行线程及逐个 SDK 回调线程；它不把应用改成前台，也不开放其他界面命令。宿主生命周期包含安全随机身份，进程重建后即使计数重用，旧审阅仍失效。前台恢复立即撤销旧后台许可；迟到 SDK、状态更新与完成回执重新核对原授权/实例。已在后台取得 OS owner 的作业不会被更晚的 Activity 退出事件误当 UI 任务关闭。

同一生产 SDK 支持真实网络、隔离存储、日志、任务、已支持的密码学及托管订阅。安全存储只经已有 Android Keystore 包装通道读取，不在后台创建或替换主密钥；不可读时拒绝，不改成普通文件。后台不弹出浏览器、文件选择或通知授权；没有可用交付适配器的交互明确失败。插件文件票据、浏览器返回、受保护运行加载仍保留原缺口。

托管订阅使用完整解析、来源归属、规则准备与关系 CAS，仅保存本地材料及待应用配置；完成同步不等于启用配置或接管 VPN。无原生运行确认时，删除正在使用的托管配置明确拒绝，不能自动停 VPN 或声称成功。非活动归属资源可按生产关系事务删除。

## 停止、幂等与观测

系统 `onStopJob`、时间预算和服务销毁先封闭本次 run 的回执，再设置真实 VM/SDK 取消标志，停止后续排队目标。在途网络请求可能排空，不能承诺已撤回。正常返回的明确失败才进入原有有界退避；持久 dispatch 后缺失回执为 `unconfirmed`，不会自动换号重放或继续新周期。执行前停止的排队记录可恢复为 cancelled。共享 [lastAttempt 契约](mobile-plugin-tasks-v0.0.3.md) 保持。

原生 AtomicFile 只保留最近 OS 执行及系统登记，预算 16 KiB，包含真实开始/观察/结束时间、停止原因及实际有回执的计数。进程重建将旧 running 标为结果未确认；停止后晚到回执不能覆盖它。尚未产生实际计数显示 `—`，不能补零。公开 Rust DTO 不向界面输出内部 process token、插件结果正文、秘密或异常原文。

“更多 → 插件 → Android 后台任务”可重新核对系统登记和最近实际执行；任务成功/失败与配置待应用分开表达。普通读状态不创建任务、不要求 VPN 或新的系统许可。

## 验证与待验

宿主测试覆盖真实无 AppHandle 的签名插件 VM/SDK、取消前 dispatch 屏障、失败和 namespace 发布、SQLite 材料持久化/重建、归属与撤权、活动资源删除拒绝、线程许可/前台恢复/生命周期失效。JVM 测试覆盖停止与晚到回执门禁。Instrumentation 使用真实 JobScheduler、PackageManager 和 AtomicFile 核对登记/恢复，但是否实际运行必须看逐提交记录；编译 test APK 不计作运行。

尚需 Android 设备覆盖：完整 App 冷启动 JNI→VM→SDK，前后台/锁屏、系统停止、进程重建、重启、断网/恢复、权限撤销、存储不足和 OEM 行为。API24/Galaxy S8 与 API30 x86 模拟器分别记录。当前云端没有已装 emulator/system image 或设备，本任务不操作用户 Mac。

依据：[JobScheduler](https://developer.android.com/reference/android/app/job/JobScheduler)、[JobService](https://developer.android.com/reference/android/app/job/JobService)、[JobInfo.Builder 周期](https://developer.android.com/reference/android/app/job/JobInfo.Builder#setPeriodic(long))、[Doze 与 App Standby](https://developer.android.com/training/monitoring-device-state/doze-standby)。

Android14+ 网络约束权限要求：[官方行为变化](https://developer.android.com/about/versions/14/behavior-changes-14)。
