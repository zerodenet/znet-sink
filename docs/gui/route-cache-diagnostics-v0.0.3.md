# 路由评估与独立 DNS 缓存

本批对应 PRD V3 A19 的两个客户端缺口；复用现有 tool jobs、权限、队列、预算和作用域，不新增 UI 私有执行队列，也不修改固定 core。

## 真实路由评估

`diagnostics.trace_route` 在实际 Proxy 中创建带端口、TCP/UDP 与可选入站标签的 Session，调用当前路由评估链。返回的 `CommandResponse` 包含目标、端口、协议、有效模式、路由动作及可选匹配规则。它不是 ICMP traceroute，不返回逐跳 TTL/RTT，也不证明目标可达。

Rust 先验证 accepted 与实际字段，再投影允许字段；规则索引以十进制字符串传至 UI，最大 u64 不经过 JS number。direct/reject/route 三种动作分别显示，没有匹配规则与接口格式未适配分别表达。UI 不从路由目标推断自动组最终叶子、解析候选或完整出口链；当前 wire 没有提供这些字段，页面明确说明。

协议、端口在创建任务前校验；入站标签由用户显式填写。跳转页面不停止共享任务，配置或实例变化使旧结果失效。复制回执只属于原结果及原作用域。

## 独立 DNS 缓存入口

DNS 工具新增独立缓存表单和任务，调用 `diagnostics.dns_cache`，不先执行 DNS 查询或 Fake-IP 查询来制造缓存。域名查询与有界列表分别呈现；limit 为 1–500。未启用、查询未命中、有效空列表、读取失败和格式不完整各有明确反馈，不将未知计数或 TTL 填零。

Rust 对地址、条目数、命中与 TTL 的关系作结构检查，关闭未知字段出口。TTL 以无损十进制字符串传递。Fake-IP 查询不再附带隐式缓存读取；既有明确 Fake-IP 清理后的同作用域缓存刷新仍保留。

## 验证与边界

新增 Node 脚本 `scripts/test-route-cache.mjs`、真实 Panel 的 320px 浏览器 fixture，以及 Rust wire/实际 Unix IPC 专项。准确执行结果记录在[验证日志](../verification/mobile-client-v0.0.3.md)和同一 Draft PR 的最终提交证据中；源码存在不等于测试已运行。

浏览器 fixture 只验证客户端交互，不替代真实手机网络、后台、权限与系统分享验收。原生 VPN/core 接入仍为明确后置项；本批不声称 A19 全部完成，不开放猜测接口或生成模拟网络采样。
