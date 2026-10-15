# Packet 路径分页与关闭屏障

Packet 页面继续使用锁定版本 zero-api 的 `packet_routes`、`packet_route` 和
`packet_routes.close`。未新增 core 接口，没有用 Flow 或端点状态代替原生 Packet 会话。

## 有界实时分页

`gui_packet_routes_snapshot` 接收可选 query；省略时从第一页开始。每页 1–500 条，
实际下发 offset/limit，返回实际 total/nextOffset。一次只保留一页，不累积无限记录。
offset 与 total 的客户端范围为 u32；越界或不一致响应明确失败，不截断。
配置 revision、endpoint generation 及时间戳继续使用十进制 u64 字符串。

后续页固定原 profile、实例、配置 revision 与客户端 scopeGeneration；宿主读取前后
核对客户端身份，服务读取前后核对真实内核身份。非空页必须位于 total 内，nextOffset
须准确前进；offset 已超过 total 时允许真实空页。每页均是一次实时读取，资源增减可能
改变后续页内容，不能把各页称作跨页冻结快照。

翻页清除旧详情和确认目标。失败保留上一页并标为过期、禁用控制；重试读取原失败页。
搜索明确只筛选当前页。刷新读取当前页，明确返回第一页才放弃旧分页锚点。

## 关闭与核对

关闭仍固定原 route ID、core instance 和 config revision，使用既有内核前置条件。
客户端代次用于使旧页面与确认失效，不代替内核的条件命令。

宿主进程持有最多 128 个物理目标的关闭账本，键为实例、route ID 和创建 revision。
发命令前预留目标；离页、翻页、列表缺行或配置 A→B→A 均不解除屏障。预算满时只读与
翻页仍可用，新关闭受限并说明原因。它不是重试队列，不自动重放命令。

明确拒绝可解除本次预留；超时、断联、缺回执或不兼容回执保留未知结果。只有固定原目标
的实际已结束事实，或同一稳定实例对该 ID 的权威不存在结果，才可协调该屏障。一般
读取失败、身份变化和分页缺行均不能代替这些事实。

账本寿命是当前宿主进程，支持 Panel 卸载重建；本批没有新增进程重启后的持久恢复账本。
原生移动服务尚未装配时继续拒绝 Packet 关闭，不把桌面 IPC 检查当作手机转发验收。

具体检查与最终提交 CI 见[移动验证记录](../verification/mobile-client-v0.0.3.md)。
