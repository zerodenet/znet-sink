# 有界捕获与固定脱敏诊断导出

此实现补齐 PRD 13.2/A19 的客户端捕获预算与系统诊断材料交付链。它使用实际已存 IPC、日志记录和共享 probe/tool 队列，不生成网络采样、PCAP 或移动 VPN 成功状态。

## 捕获所有者与预算

`models/debug/capture.rs` 的进程级宿主会话拥有捕获状态；`models/debug/worker.rs` 负责有界队列入盘。原生入口接受整数时长 1–300 秒、总容量 1–10 MiB。旧桌面 `gui_debug_capture {enabled}` 参数保持兼容，默认最多五分钟或 10 MiB；读状态不重新开始或延长会话。

每次会话生成随机 opaque ID，绑定原生入口取得的 ClientScope。`AppState::commit_diagnostic_capture` 在同一个 client_core mutex 内检查预期 scope 并提交捕获，防止授权 lease 等待期间变化后启动旧 scope。配置提交、实例启动和失联的中央 hooks 使旧会话停止；停止命令检查原 session ID，不会停止后建立的会话。

状态包含开始/截止/结束时间、预算、实际存储字节与帧数、观察到的队列丢弃数和停止原因：duration、capacity、user、cleanup、storage_failure、scope_changed。时长同时使用 monotonic deadline，系统时钟回拨不能延长会话。截止属于宿主 lease，入口读状态和后台生产/消费都会判断；不依赖页面 timer。没有新 IPC 时不会产生虚假采样。

借用原 JSON 的已有 projection 继续限制单帧字节、节点、深度和字符串，遮盖配置、凭据字段与 URL。生产者记录会话 ID，worker 在入盘前再次检查会话和截止。总预算计入实际序列化帧及换行；只有存储成功才增加 storedBytes/storedFrames。下一帧无法装入剩余预算时停止详细捕获，不超额写入。过期/失效/停止后的排队详细帧降为既有摘要；连接完成记录依旧进入独立连接历史。

队列保持 128 项，生产线程不阻塞。queueDropsObserved 是会话期间观察到的共享 IPC worker 排队损失，不声称精确定位到每个资源。调试存储仍按五个 1 MiB 段轮换；session lifetime 写入计数不等于所有帧都永久保留，导出也不承诺完整无限历史。

当前捕获会话是进程内状态，进程重建不恢复或重放旧捕获。新进程显示无执行中会话，已存历史可读；上一进程会话的最终停止原因未持久核对，不应宣称其正常完成。这仍是捕获历史生命周期补充项。

## 清理与兼容性

`gui_debug_clear` 仅接受默认/旧 all/debug-capture（捕获数据）或明确 connection-history（连接历史）；未知 scope 拒绝。旧 DebugTab 默认清理调用曾隐含删除连接历史，本次修正为只清理调试捕获；连接页面及移动本地数据的明确历史操作继续保留。

捕获清理持 RAII cleanup gate，先停止详细会话，更新 storage epoch，再等待 worker barrier，最后删除调试段。协调与删除期间新捕获启动被拒绝。生产于清理前、却晚入队的帧不能重建已清理调试记录；连接历史仍独立接收真实完成事件。barrier 的排队和确认共用 30 秒上限，失败传播，不把写入仍未结束说成清理成功。部分文件删除失败报告实际已删除段及未核对范围，不声称所有旧数据仍完整。默认摘要观测可在清理后继续产生新的记录，清理不终止共享 IPC 或转发。

## 宿主固定快照

`services/diagnostics.rs` 准备最多 2 MiB 的 JSON 文件，只读取：

- 最多最近 1000 条实际日志的来源、级别、时间、ID，以及宿主分类的固定 action/outcome；任意日志字段（包括数字）默认排除。
- 实际 debug_store 的有界页（最多 512 KiB），包含帧时间、方向、类别、耗时、错误存在事实和截断标记。只有宿主已绑定遥测来源的成功统计回执或规范统计事件导出安全计量结构；Flow/Inner/Outer/Host 平面保留各自结构，同名计量不合并。
- 当前 ClientScope 的共享 probe/tool 任务类型、状态、真实完成/成功/失败/目标数量及生命周期时间。不创建 UI 私有任务队列。
- 实际客户端 source 状态、平台契约和捕获回执。实时 Proxy/kernel 健康未读取时明确省略，不以验证组件状态冒充。

`DebugFrame.telemetry` 是宿主内部类型字段，历史记录缺省为 None。protocol 在核对返回 ID 后，从原始且单一的 stats/traffic_stat/traffic_stats QueryRequest 分配查询来源；connection reader 只对规范 zero.event.v1 的 stats.sampled/scopes_sampled/host_interfaces_sampled 事件分配事件来源。请求/命令正文、配置及诊断响应不能通过自报 method 或 payload marker 取得来源。导出还核对接收方向、成功回执、对应 result 分支或事件类型；无来源、失败、形状冲突和有界副本缺计量分别保留明确省略原因及数量，旧帧只导出元数据，不视为零。借用的 typed event 解析忽略原正文，来源判定不复制任意大 payload。

所有普通 `logs::append_entry` 调用（包括公共 append、guest SDK、IPC 与内核记录）移除输入里的保留 provenance marker；私有可信 writer 仅由 `plugin_host_event` 调用并写入固定材料分类。导出只保留有限 action/outcome 枚举，动态 action、SDK参数、错误与身份不进入该分类。guest SDK 字段仍嵌套，不能冒充宿主分类。

自由消息、任意错误正文、unknown fields、目标标签、subject、params、result、协议私有材料和未经确认的插件组件身份不具备普遍无秘密保证，默认排除。配置正文、订阅 URL/headers/token、凭据与插件秘密不会交付。每个日志来源分别列出省略数量与原因。该材料包含实际安全公共事实，但不称作完整日志正文导出；若要扩展消息，需要在可信宿主 writer 引入可证明安全的事件材料分类，不能凭关键词扫描认定任意消息安全。

计量字符串 u64 保持十进制字符串，不转为 JS number。历史标为 bounded_client_storage_across_instances，不能据当前 profile 标题断言所有历史帧属于当前内核或配置。帧读取失败显示数量未知及原因，不能展示“真实空记录”。输入先受每域读取预算约束，最终 JSON writer 再限制 2 MiB；超限明确失败，不交付声称完整的截断文件。

准备结果只给 UI plan ID、scope、数量、文件字节与省略说明，不把原始材料交给 WebView。宿主缓存最多四份快照、五分钟有效。交付核对原 ClientScope 和期限，使用准备时的字节，不重新读取扩大范围。每个计划在系统调用前单次消费，超时/未知不自动重放；取消可重新明确预览，原存储资料不变。新进程旧 plan 不存在，需重新预览。

## 系统交付与 UI

新 `native_document_preview_diagnostics` 和 `native_diagnostic_capture` 仅宿主 main 窗口可调用；capture 控制复用 diagnostic.capture capability lease。既有 native_document_export_diagnostics 增加可选 planId 以兼容旧显式调用，新移动 UI 始终提交固定 planId。

`native_documents::deliver_prepared` 复用 Native IO 的 host-owned PreparedExport、用途/容量校验、系统 save/share 与回执转换。saved、share-accepted、cancelled、failed、result-unknown 保持区别；分享面板接受不等于接收方保存，未知先核对，不自动重发。

`NativeDiagnosticExport.svelte` 提供预算表单、原会话停止与手动状态核对、快照范围和省略预览、保存/分享。页面退出不停止捕获；恢复前台读取权威状态；显示数值附最后读取时间，不用网页计时器补齐进度。原生入口不提供桌面路径、URL token 或任意文件执行。

## 验证与未运行项

Rust 测试覆盖预算边界、实际入盘计数、容量截止、失效/旧会话控制、单调时长、队列与 barrier、部分清理及数据范围隔离、未知 scope、配置/实例及期限拒绝固定导出、single delivery、巨大文件拒绝、秘密 canary、任意配置/诊断数字字段及自报方法拒绝、旧帧兼容与宿主来源绑定的不同统计平面保留。Session 模型测试使用独立实例，避免全 workspace 配置 hooks 污染 global capture。

`test-native-diagnostics.mjs` 验证产品输入范围、不同停止原因和真实 Tauri invokes 的原 plan/session 参数。独立 Chromium 测试验证 320 px 表单、输入无效零写入、停止原会话、预览固定快照与结果未知后禁止重放。`pnpm check` 与共享 UI 控件检查验证类型/控件契约；最终 Rust/全量前端结果由主交付报告记录。

尚未完成 Android/iOS 真机后台、锁屏、磁盘不足、系统文档授权撤销与分享验收；移动真实 Proxy/外部 TUN 数据面仍受主报告的跨仓库依赖约束。不会据这些客户端测试宣称真机 VPN、完整 A19 或全部移动端验收完成。
