# 移动端订阅前台到期协调

本实现处理实际原生宿主启动和恢复前台时的到期检查。没有注册 WorkManager、BGAppRefresh 或成功返回的空 worker；锁屏/睡眠期间的后台调度仍未适配，也不承诺某个时刻执行。iOS 主应用的标准生成、完整 SDK 检查和 UI 烟测已单独纳入 [宿主批次](ios-ui-host-v0.0.3.md)，以实际 CI 回执为准；本节的服务测试不替代签名、安装或完整 iPhone 生命周期验收。

## 实际入口与分层

`mobile_host/application.rs` 的 Tauri `Ready`（冷启动）及原生 `WindowEvent::Resumed` 调用 `mobile_subscription_scheduler::resume`；`Suspended`、退出/请求退出调用 `suspend`。当前锁定 Tauri 源码将 Android `Activity.onResume/onPause` 和 iOS UIApplicationDelegate 恢复/失活转成这些 WindowEvent；Android 首次 onResume 被 Tauri 忽略，所以保留冷启动入口。入口不依赖网页 visibility、JavaScript timer、页面是否打开或刷新按钮。仅原生宿主注册，桌面窄屏预览的 snapshot 为 `not-native-host`。

AppState 持有共享协调状态。每个前台 epoch 只有一个 pass；重复回调在已有 pass 时合并。进入后台改变 epoch，阻止后续排队目标；已有共享工作流可能继续返回或被系统暂停。返回前台时若旧 pass 仍在途，等其真实结束再处理新 epoch。此处不宣称撤回已发出的 HTTP 或核心请求。

## 复用的执行链与边界

`subscription_wrapper::scheduled_profiles` 先复用既有格式/User-Agent 迁移，再取存储快照。目标只包括普通 URL 来源、已启用参与同步、周期至少 60 秒且按真实 lastSync 到期的订阅；插件托管源、手动源、禁用和非法周期不进入队列。

新增 host-only `subscription::sync_due` 与手动同步共用 `subscription_syncs` 每 ID guard，并在 guard 内重检：来源 URL、格式、User-Agent、kernel、关联配置、周期、updatedAt 与 lastSync。手动同步已完成、源被编辑、目标已移除或正在同步都会跳过。没有第二份 HTTP/解析/配置应用实现：之后仍进入 `sync_guarded -> sync_subscription -> client capability network.get -> 原生解析/规则集材料 -> 既有配置发布 -> 实际同步成功记录`。

每次 pass 最多发起 8 项，45 秒是发起下一目标前的预算；单项既有工作流包含有界 HTTP/规则集/配置阶段，可能超过 pass 预算，不能假称已强制取消。长列表按最近实际尝试时间轮换，未执行项在后续真实恢复事件核对，没有内部周期 timer。网络错误使用共享 capability executor 的既有去重、预算、权限和脱敏错误。

下载/同步材料与运行配置确认不同。默认新材料不自动启用；当前移动运行未接入时仍沿既有离线配置保存链，不能将本地材料更新称作 VPN/运行配置已生效。运行/存储/其他结果不明确的错误作为 `result-unknown`，不靠重新下载掩盖未知提交。

## 恢复记录与失败

`builtin.scheduler.mobile-subscriptions` 使用既有 `schedule_store` capability 私有 namespace、SQLite revision/CAS，记录最多 64 项。不把 URL、源名称、请求头、token、原文、原生 exception 写入调度记录或事件；事件只有合法本地 ID/固定状态码。

请求前先持久化 `in-progress`；记录保存失败不会发起请求。每项结束后依据共享工作流实际返回与 lastSync 记录 confirmed/failed/skipped/unknown，再持久化；保存回执失败保留未知屏障。进程恢复将未完成记录解释为 `result-unknown`，不会自动换 ID 重放。只有读取到 lastSync 已实际变化等确认事实才能解除；关闭自动意图再重新打开不会删除未知屏障。满额未知记录不被驱逐来启动无记录请求。

已确认网络/源拒绝失败采用最多四次一轮的退避（60/120/240 秒后再由真实前台事件触发，耗尽后至少 15 分钟或配置周期的冷却），不承诺后台重试。没有可执行的新前台事件时不会自行醒来。损坏/未来 checkpoint、存储不可读或写失败暂停该 pass，读取失败保留旧可读记录并注明存储错误。

普通 URL 订阅已复用共享条件请求执行器和私有 validator/material 记录：只有来源、成功同步 revision 和仍有效的关联材料完全匹配时，304 才更新检查时间；不应用或覆盖配置。意外 304、记录损坏、源/材料变化及未知提交均保留旧有效材料并明确反馈。详见 [普通订阅条件同步](ordinary-subscription-conditional-sync-v0.0.3.md)。

## 验证与尚未运行

- 调度 policy tests：真实 epoch 去重、后台停止排队、新 epoch 等待、进程恢复未知屏障、关闭/重开不清屏障、due 过滤、容量、有限退避、无敏感记录。
- 私有 SQLite storage 实测 reopen：同 native capability namespace 写入/重读在途记录，恢复未知且不发起新 due 目标。
- 既有 subscription capability workflow 实际 localhost HTTP 测试：200 元数据和统一操作记录；有效200→304条件请求/意外304拒绝/旧具体配置不变/URL token 不进入错误和操作记录；同毫秒源变更及手动成功使旧预检失效。
- TypeScript 文案测试：由权威 `foregroundReconciliation` 区分 native foreground 与未适配平台，始终保留系统后台调度未适配事实；小于 60 秒/不安全周期不执行。

以上单元、服务和编译检查不替代 Android S8/API24 或 iOS 真机冷启动、锁屏、系统暂停、进程杀死、网络切换及存储不足验收。当前完整域同步 API 仍依赖 AppHandle（原生状态与事件/配置编排）；WorkManager/Apple 后台任务不能在无宿主时调用一个空壳或 UI invoke 来声称成功。真实后台方案需要独立可初始化的域执行上下文、平台截止/取消和恢复协调，属于未适配项。
