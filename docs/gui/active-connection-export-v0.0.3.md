# 活动连接的固定筛选原生导出

2026-10-14，客户端 PRD V3 A18/C10；仅修改客户端，不修改或推送 core。

活动页的「导出筛选活动连接」固定当前搜索、协议和出口筛选。它请求宿主读取一次
真实 `active_flows`，不上传页面记录、活动 ID 集合、配置正文或自报数据来源。
暂停仅冻结页面展示；导出预览仍读取当前内核，并在面板明确说明这一点。
完成历史继续使用独立的现有本地历史预览；桌面导出行为保持原有路径。

宿主通过现有 `BoundControl` 固定同一已认证 IPC 连接，先核对 capabilities V1、
control API V1、`runtime_snapshot`/`flow_snapshot` 和 runtime identity，再使用
`zero-api::QueryRequest::ActiveFlows`/`FlowListQuery`、`FlowSnapshot` 解析实际返回值。
查询的权限为 Read，由实际核心查询鉴权；`capabilities.permissions` 是元数据，不能
代替本次调用的授权。缺少完整 canonical record 时拒绝预览，不用默认零补齐计量。

应用搜索和出口筛选前先读取最多 2001 个实际对象；第 2001 个仅表明查询窗口可能
截断，实际材料最多 2000 条。查询期间实例或配置版本变化、格式不兼容、重复身份、
权限拒绝或传输失败均拒绝预览。窗口外是否还有匹配记录保持未知，不承诺导出无限库存。
导出只包含校验后的连接 ID、IP/套接字、DNS 域名、真实时钟/计数和状态。
认证材料、URL userinfo/query/fragment、协议秘密、进程路径、规则文本和自由错误均不导出。
地址、域名和连接身份仍有隐私影响，面板明确提示并要求确认。

预览材料由宿主固定，携带真实内核实例、配置 revision、读取时间和筛选摘要。
全部活动/历史预览合计最多 4 个，每个材料上限 2 MiB，有效期 5 分钟；仅在进程内保存。
交付前重新核对 ClientScope、控制绑定、同一连接的能力和 runtime identity。
此时不重新读取 active_flows，不添加新连接、不更新原计数。通过后单次消费材料，
再交给现有 host-generated/redacted-diagnostic 原生 IO 策略。等待系统选择或分享时不
持有配置图锁。取消、分享已接受、失败及结果未知沿用实际系统回执，不自动重放。
进程退出使预览失效；重新预览是用户明确操作，不恢复旧系统请求。

当前 Android/iOS 运行服务/扩展控制桥尚未接入，活动预览返回结构化 unsupported
原因；不能改用桌面 IPC、Engine 控制布尔值或模拟连接。完成历史的真实本地快照导出
不依赖移动运行态，因此保留可用。本工作没有完成手机活动连接或真实原生分享验收。

实现位于 `services/connection_export/{active,ipc_tests}.rs`、相应 Tauri 命令与
`features/connection-export`、`ConnectionExportDialog`、`ConnectionInspectorWorkspace`。
隔离 Unix IPC 测试使用真实客户端传输与公开 DTO；浏览器 gateway 是明确测试夹具，
不会进入产品路径。验证计数以父代理整合记录和本次日志为准，不以测试 peer 称真机验收。
