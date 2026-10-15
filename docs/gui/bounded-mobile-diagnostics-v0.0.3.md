# 有界捕获与固定脱敏诊断导出

此实现补齐 PRD 13.2/A19 的客户端捕获预算与系统诊断材料交付链。它使用实际已存 IPC、日志记录和共享 probe/tool 队列，不生成网络采样、PCAP 或移动 VPN 成功状态。

## 捕获所有者与预算

`models/debug/capture.rs` 的进程级宿主会话拥有捕获状态；`models/debug/worker.rs` 负责有界队列入盘。原生入口接受整数时长 1–300 秒、总容量 1–10 MiB。旧桌面 `gui_debug_capture {enabled}` 参数保持兼容，默认最多五分钟或 10 MiB；读状态不重新开始或延长会话。

每次会话生成随机 opaque ID，绑定原生入口取得的 ClientScope。`AppState::commit_diagnostic_capture` 在同一个 client_core mutex 内检查预期 scope 并提交捕获，防止授权 lease 等待期间变化后启动旧 scope。配置提交、实例启动和失联的中央 hooks 使旧会话停止；停止命令检查原 session ID，不会停止后建立的会话。

状态包含开始/截止/结束时间、预算、实际存储字节与帧数、观察到的队列丢弃数和停止原因：duration、capacity、user、cleanup、storage_failure、scope_changed。时长同时使用 monotonic deadline，系统时钟回拨不能延长会话。截止属于宿主 lease，入口读状态和后台生产/消费都会判断；不依赖页面 timer。没有新 IPC 时不会产生虚假采样。

借用原 JSON 的已有 projection 继续限制单帧字节、节点、深度和字符串，遮盖配置、凭据字段与 URL。生产者记录会话 ID，worker 在入盘前再次检查会话和截止。总预算计入实际序列化帧及换行；只有存储成功才增加 storedBytes/storedFrames。下一帧无法装入剩余预算时停止详细捕获，不超额写入。过期/失效/停止后的排队详细帧降为既有摘要；连接完成记录依旧进入独立连接历史。

队列保持 128 项，生产线程不阻塞。queueDropsObserved 是会话期间观察到的共享 IPC worker 排队损失，不声称精确定位到每个资源。调试存储仍按五个 1 MiB 段轮换；session lifetime 写入计数不等于所有帧都永久保留，导出也不承诺完整无限历史。

捕获恢复账本独立写入应用私有目录的 `capture-recovery.json`，schema v1，最多32项、64KiB；只存会话身份、ClientScope、阶段、确认状态、预算/截止/结束时间及实际计数，不含 IPC 正文、日志消息、参数、错误、目标标签、URL 或插件秘密。每次开始先原子存盘再开放详细捕获；用户停止、预算截止、配置/实例失效及存储故障记录终态。Unix 检查私有文件及目录同步；同步失败即使替换已可见也报告结果未确认，不声称旧字节仍保留。

正常计数检查点由现有 debug worker 在捕获 mutex 内生成有界快照后，释放捕获 mutex 再存盘，不增加业务队列。进程内单调顺序防止迟到的计数快照覆盖更新的停止或新会话；旧 scope/session 不能控制替代会话。健康 IPC 生产者只读轻量状态，不同步写每帧计数，也不复制整份恢复历史。开始/停止与截止等稀有状态提交仍按原确认边界同步保存，不承诺磁盘故障时无 IO 等待。

每次平台 setup 在 IPC 生产者启动前读取账本。上一进程遗留 capturing 的记录只投影为 interrupted/process_restart/result_unknown，无确认结束时间；即使壁钟已超过原截止，也不能推断其正常达到预算。该保守投影存盘成功表示账本已保存，绝不把任务最终结果改为成功；确认停止历史保持原状态和原因。进程重建没有 active deadline，不恢复或重放捕获。历史计数是最后成功持久检查点，不能据此宣称完整捕获。用户可显式开始新会话，生成新身份并使用当前作用域。

开始、停止、计数或恢复投影的存盘失败会停止或阻止详细捕获，返回安全错误或在权威状态中显示 journal_failure/recoveryState=result_unknown；原操作不自动重复提交。损坏、超预算或新版本账本不回退为空结果，不自动覆盖，未知字段也拒绝。显式的捕获所属清理成功后才解除该阻塞。界面保留恢复记录、未确认结束时间和原因；恢复前台只重读宿主状态，不使用网页 timer 补全结果。

## 清理与兼容性

`gui_debug_clear` 仅接受默认/旧 all/debug-capture（捕获数据）或明确 connection-history（连接历史）；未知 scope 拒绝。旧 DebugTab 默认清理调用曾隐含删除连接历史，本次修正为只清理调试捕获；连接页面及移动本地数据的明确历史操作继续保留。

捕获清理持 RAII cleanup gate，先停止详细会话，更新 storage epoch，再等待 worker barrier，最后删除调试段并原子清空捕获所属恢复账本。账本清理不遍历记录中的身份或路径；日志、连接历史、配置和其他数据不在该范围。帧已删除而账本清理失败时报告实际部分完成，不声称已全部清理或原帧仍在。协调与删除期间新捕获启动被拒绝。生产于清理前、却晚入队的帧不能重建已清理调试记录；连接历史仍独立接收真实完成事件。barrier 的排队和确认共用 30 秒上限，失败传播，不把写入仍未结束说成清理成功。部分文件删除失败报告实际已删除段及未核对范围，不声称所有旧数据仍完整。默认摘要观测可在清理后继续产生新的记录，清理不终止共享 IPC 或转发。

## 宿主固定快照

`services/diagnostics.rs` 准备最多 2 MiB 的 JSON 文件，只读取：

- 最多最近 1000 条实际日志的来源、级别、时间、ID，以及宿主封闭事件类型和由该类型重建的固定安全文案；原消息与任意日志字段（包括数字）仍排除。
- 实际 debug_store 的有界页（最多 512 KiB），包含帧时间、方向、类别、耗时、错误存在事实和截断标记。只有宿主已绑定遥测来源的成功统计回执或规范统计事件导出安全计量结构；Flow/Inner/Outer/Host 平面保留各自结构，同名计量不合并。
- 当前 ClientScope 的共享 probe/tool 任务类型、状态、真实完成/成功/失败/目标数量及生命周期时间。不创建 UI 私有任务队列。
- 实际客户端 source 状态、平台契约和捕获回执。实时 Proxy/kernel 健康未读取时明确省略，不以验证组件状态冒充。

`DebugFrame.telemetry` 是宿主内部类型字段，历史记录缺省为 None。protocol 在核对返回 ID 后，从原始且单一的 stats/traffic_stat/traffic_stats QueryRequest 分配查询来源；connection reader 只对规范 zero.event.v1 的 stats.sampled/scopes_sampled/host_interfaces_sampled 事件分配事件来源。请求/命令正文、配置及诊断响应不能通过自报 method 或 payload marker 取得来源。导出还核对接收方向、成功回执、对应 result 分支或事件类型；无来源、失败、形状冲突和有界副本缺计量分别保留明确省略原因及数量，旧帧只导出元数据，不视为零。借用的 typed event 解析忽略原正文，来源判定不复制任意大 payload。

诊断文件 schema 为 `znet.mobile-diagnostics.v3`。`LogEntry.diagnosticEvent` 是宿主内部的封闭枚举，不含自由字符串、任意 JSON、身份、路径或 URL；`LogAppend` 不接受此字段。普通 append、guest SDK、IPC 与内核记录的输入不能给该字段赋值，旧保留 marker 仍移除。只有实际宿主分支通过类型化 writer 选择事件，包含进程创建/就绪/停止/监测、订阅与规则调度、规则依赖、节点快照、测速、旧连接清理以及插件宿主操作。事件记录历史事实，不授予控制权限、不代替当前内核/VPN 健康。

导出核对事件预期来源，`safeMessage` 完全从有限枚举重建；不读取原 `message` 或字段来拼接。动态动作、SDK 参数、错误、进程参数、目标和未经确认的组件身份都不进入安全正文。原消息仍标 `messageOmitted:true`，缺少或来源不符的分类给出省略原因。测速的请求未发送/本地失败与远端结果区分，不把权限错误或未取得结果改为节点不可达。

分类随现有应用私有 JSONL 保存、分页、轮转和重开，不建新的业务队列；旧日志没有分类，旧 marker 不升级为可信事件。未知、畸形或带额外字段的分类只降为无分类，保留其余日志元数据；不会因新版本事件让整页不可读。该信任边界是宿主拥有的私有存储，不提供对可直接修改磁盘主体的密码学真实性保证，也不使用历史分类控制新实例。

自由消息、任意错误正文、unknown fields、目标标签、subject、params、result、协议私有材料和未经确认的插件组件身份不具备普遍无秘密保证，默认排除。配置正文、订阅 URL/headers/token、凭据与插件秘密不会交付。每个日志来源分别列出省略数量与原因。新增固定文案只覆盖已接入的宿主事件，仍不称作完整日志正文导出；扩展分类必须新增可审阅的类型与实际 writer，不能凭关键词扫描认定任意消息安全。

计量字符串 u64 保持十进制字符串，不转为 JS number。历史标为 bounded_client_storage_across_instances，不能据当前 profile 标题断言所有历史帧属于当前内核或配置。帧读取失败显示数量未知及原因，不能展示“真实空记录”。输入先受每域读取预算约束，最终 JSON writer 再限制 2 MiB；超限明确失败，不交付声称完整的截断文件。

准备结果只给 UI plan ID、scope、数量、文件字节与省略说明，不把原始材料交给 WebView。宿主缓存最多四份快照、五分钟有效。交付核对原 ClientScope 和期限，使用准备时的字节，不重新读取扩大范围。每个计划在系统调用前单次消费，超时/未知不自动重放；取消可重新明确预览，原存储资料不变。新进程旧 plan 不存在，需重新预览。

## 系统交付与 UI

新 `native_document_preview_diagnostics` 和 `native_diagnostic_capture` 仅宿主 main 窗口可调用；capture 控制复用 diagnostic.capture capability lease。既有 native_document_export_diagnostics 增加可选 planId 以兼容旧显式调用，新移动 UI 始终提交固定 planId。

`native_documents::deliver_prepared` 复用 Native IO 的 host-owned PreparedExport、用途/容量校验、系统 save/share 与回执转换。saved、share-accepted、cancelled、failed、result-unknown 保持区别；分享面板接受不等于接收方保存，未知先核对，不自动重发。

`NativeDiagnosticExport.svelte` 提供预算表单、原会话停止与手动状态核对、快照范围和省略预览、保存/分享。页面退出不停止捕获；恢复前台读取权威状态；显示数值附最后读取时间，不用网页计时器补齐进度。原生入口不提供桌面路径、URL token 或任意文件执行。

## 验证与未运行项

Rust 测试覆盖预算边界、实际入盘计数、容量截止、失效/旧会话控制、单调时长、队列与 barrier、部分清理及数据范围隔离、未知 scope、配置/实例及期限拒绝固定导出、single delivery、恢复执行中/确认停止记录、损坏/新schema保留、恢复计数/停止/截止存盘失败、迟到检查点拒绝、32项/64KiB及私有权限、巨大文件拒绝、秘密 canary、任意配置/诊断数字字段及自报方法拒绝、旧帧兼容与宿主来源绑定的不同统计平面保留。Session 模型测试使用独立实例，避免全 workspace 配置 hooks 污染 global capture。

`test-native-diagnostics.mjs` 验证产品输入范围、不同停止原因和真实 Tauri invokes 的原 plan/session 参数。独立 Chromium 测试验证 320 px 表单、输入无效零写入、停止原会话、预览固定快照与结果未知后禁止重放，以及重建后的中断/确认历史显示、账本失败禁新会话。`pnpm check` 与共享 UI 控件检查验证类型/控件契约；最终 Rust/全量前端结果由主交付报告记录。

尚未完成 Android/iOS 真机后台、锁屏、磁盘不足、系统文档授权撤销与分享验收；移动真实 Proxy/外部 TUN 数据面仍受主报告的跨仓库依赖约束。不会据这些客户端测试宣称真机 VPN、完整 A19 或全部移动端验收完成。
