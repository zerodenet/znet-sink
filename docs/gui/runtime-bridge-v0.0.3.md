# v0.0.3 客户端运行控制边界

本页记录客户端实现，内核只读依据为 `zerodenet/core@e74561f99acca941bfadf0d33e35a5ef08f34086`。不修改内核、不创建签名凭据，也不把控制通道检查或 IPC 测试称为手机 VPN 验收。

## 实际接口与接入

内核现有 `crates/api/src/traits.rs` 定义 `QueryService`、`CommandService::execute_acknowledged`、`EventSource::subscribe/since`。`crates/proxy/src/runtime/handle/{query,event}.rs` 及 `command/dispatch.rs` 已由 `ProxyHandle` 实现；这不是新接口建议。`EngineHandle` 也实现控制 traits，但它不能证明完整 Proxy 转发、外部 TUN、外层 socket 保护或平台服务生命周期。

客户端实际桌面通路为 `kernel::connection::MultiplexedConnection` 与 `kernel::configuration::BoundControl`。`services::runtime_bridge` 使用同一条固定 IPC 连接取得真实运行身份，再通过共享 `znet-engine-client::bridge` 查询；生产诊断入口仅投影身份和操作回执元数据，排除原始运行回复、路径及错误正文。Android/iOS 没有真实 Proxy 注册时明确 unsupported；不借用桌面 socket，不生成假实例或接管状态。

## 请求、权限与结果

- `HostCaller` 只能由宿主从实际原生窗口标签产生；插件 guest 提供的标签、权限、实例或操作 ID 不是授权依据。桥不提供任意 Tauri invoke。
- `RuntimeBinding` 固定实际 core instance、运行 config revision、当前 profile 和客户端 generation。私有 epoch 阻止 invalidate 后重新绑定相同身份使旧票据复活。宿主必须在中央配置/实例变更边界 invalidation，不能仅在 UI 变更路由时更新。
- 单次 `OperationTicket` 由宿主签发，没有 Clone/Deserialize 或公开 ID 构造器。操作 ID、revision、generation 的外发值使用十进制字符串。256 条元数据回执中在途项不会被淘汰；已淘汰 ID 不意味着失败，也不能作为重新提交依据。
- 请求复用既有 client-core capability `Policy/Lease`、并发及资源预算。宿主身份、精确绑定 permission scope、客户端权限上限、实际服务端权限检查共同成立才可执行。传入的 `AuthContext` 是客户端限制，不是服务端授权证据。
- 在同一个 transport 上读取运行身份，执行请求，再核对身份、宿主绑定和 capability 交付权限。晚结果不交付给新配置、实例或 generation。取消等待/权限撤销不代表已发送的控制被撤回。
- 新窄桥开放已知查询，但不输出 `config` 原文或执行未知查询。控制仅为固定 Flow ID 的关闭，以及带实例和 revision 前置条件的 Packet route 关闭。配置应用、模式、TUN、端点、测速和任务继续由已有领域事务管理，不能绕过这些事务调用泛化控制。
- 确认关闭必须有 `accepted` 与原目标的 `closed` 事实。Flow 回执还须匹配原 flow ID；Packet 回执须匹配原 route、实例及 revision。缺失回执、超时、断联或内部错误保留 Unknown；明确验证/权限/前置条件拒绝独立记录 Rejected。没有自动重放或盲目回滚。

## 监听、预算与重同步

`gui_events_stop` 现在只终止自己的 observation generation；它不关闭共享 IPC，也不终止 pending control、另一监听者或运行服务。宿主停止进程仍有独立 `connection::reset/reset_endpoint` 路径。GUI 内部 probe/traffic 处理继续共享公共通道，页面 filter 只控制该监听者的交付。

实际 IPC 单帧上限 4 MiB，请求同时在途上限 64；重复在途 ID 拒绝，丢弃请求 future 会清理自己的等待槽。deadline 覆盖写等待和回复，已排队的原生写入可能继续，因此超时不声称取消成功。共享事件广播保留 128 项，每项最多 256 KiB；超限产生显式缺口标记，慢消费者通过 Lagged 得知丢失。格式错误与超限错误不回显原始帧或秘密。

GUI 缺口会先报告重连，再丢弃旧积压接收器并重读权威快照；原始事件监听遇到缺口停止自身交付并要求重新读取，不能静默继续应用增量。每个监听者的 source/gui 类型过滤独立验证，非法 filter 不会使原监听失效。

原始 core 事件交付复用既有脱敏规则；GUI 未知事件的 summary 与可能携带 URL 的来源/身份字段也在交付前投影。未知类型、原因、实例与精确序列等安全元数据保留，不因兼容性未知而静默隐藏事件。

共享 `bridge::events` 是面向未来真实 native owner 的有界 invalidation hint 缓冲，只保存公开事件类型和序列，不保存任意 payload、配置或凭据。它保留 128 条、每页最多 64 条；源 replay gap、缺少身份、错误实例/revision、游标越界或淘汰都要求权威快照。gap 时不交付增量。foreground rebind 每次更新 listener epoch，即使运行身份相同，旧监听也失效；单个 listener cancel 只停止交付。

该 hint 接口不替代内核现有 EventSource，也不重建客户端业务任务队列。真实 Android 服务/iOS 扩展注册、跨进程鉴权、事件源采集及真机前后台恢复仍属于未完成平台依赖，当前不声称已接入。

## 验证范围

共享桥测试覆盖可信 caller、缺少实例、权限交集、单次票据、有界回执、固定身份与 ABA、在途撤权、超时不重放、明确拒绝、目标关闭回执及 listener gap/epoch。实际 Unix IPC 测试覆盖停止真实 GUI listener 后 pending control 与新 observer 继续复用原 peer、请求数量/重复 ID/取消清理、事件超限与广播溢出、帧预算及无敏感正文回显。

Unix peer 是独立协议测试夹具；它没有启动生产 Zero、Proxy 或手机隧道。最终 workspace 检查与平台未运行项由 PR 验证记录分别列出。
