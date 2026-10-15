# PRD V3 客户端完成度与剩余能力

2026-10-15，审计基线 `19682c75dee28e3a11f45b805e5b25b0268f85c7`，同一 Draft PR #46。
先完成下表的只读审计，再选择本轮A08有限增量；基线测试在该 SHA 的 CI 执行过，不代表本轮重新执行，
也不表示整个 A 项或真实手机验收完成。原生 VPN、内核接入及宿主生命周期装配是用户明确后置；
不能把插件、安装器或其它未完成用户功能一并说成已被用户后置。

| PRD | 已有客户端链及证据 | 可独立继续的用户能力 | 明确后置或外部条件 |
| --- | --- | --- | --- |
| A01 | `MobileShell.svelte`、`mobile-shell.spec.ts`：四底栏、返回、保留页面与滚动、安全区 | 未发现需要另造导航框架的缺口 | 真机字号/键盘/旧 WebView及Galaxy S8验收 |
| A02 | `MobileOverview.statistics()`、`mobile-overview.spec.ts`：唯一顶部全局Flow入口及返回 | 已有客户端链，不重复实施 | 手机布局验收 |
| A03 | 移动生命周期模型、真实unsupported原因 | 不用UI布尔值补假VPN | **已后置**原生服务/扩展、授权/撤权及转发 |
| A04 | 模式/目标提交及拒绝保留、移动概览覆盖 | 当前配置/出口入口只切tab未定位；规则模式不能用globalOutbound充当唯一有效出口 | 实际移动运行通道 |
| A05 | `MobileNodesList`、20组/嵌套与selector/自动组测试 | 未发现需要重写组模型的缺口 | 手机协议及运行服务 |
| A06 | `mobile-view.ts`：独立节点、搜索、离线过滤、仅自动组排序 | 已有客户端链 | 真机长列表体验 |
| A07 | probe jobs、失败项重试、draining及scope失效测试 | 已有客户端链，不另建队列 | **已后置**服务执行/进程恢复与真实网络场景 |
| A08 | 基线有文件/导出及删除预览；本轮补`profile_editor` Rust文本快照/校验/本地CAS、`ProfileEditor`及真实组件测试，修复普通添加/编辑JS往返损坏u64；新建保持未启用，已有选择不变 | 仅普通内嵌Zero来源；订阅来源/客户端覆盖仍分开。path-only及受保护来源不降级，未授权规则集/GeoIP依赖明确拒绝。详见[本轮契约](profile-editor-lossless-v0.0.3.md) | 真实系统文件提供者、依赖授权/目录语义、实际编译协议与运行应用仍待验收；不称整个A08完成 |
| A09 | `SubscriptionsTab`、条件同步/304/UA/格式/周期/关联、subscriptions浏览器及capability测试 | 本次未找到优先于A08的新独立入口缺项 | 系统后台调度尚未适配；不能承诺准点，不能全部归core |
| A10 | `RulesTab`规则集CRUD/刷新/公共绑定，`rule_overlay`及`route_integrity` | **缺route.rules CRUD/顺序/启停；删除规则集缺引用位置/迁移。** 原生无enabled/协议条件字段，须设计真实profile覆盖而非改标题 | 运行和手机场景仍需验收 |
| A11 | 工作区三视图、原生校验、无损文本、离线保存、原操作有限本地完成；workspace/recovery/browser测试 | 冻结材料/依赖/所有权/watchdog仍属客户端工作；实际依赖准入与完整安全通道未完成 | 实际Proxy启动回执缺失；不开放Restart生效入口 |
| A12 | TUN/UDP/测速参数及四表单草稿保护 | 移动运行网络入口仅UDP超时；MTU仍在TUN页，本地监听及其他网络条件字段入口不完整 | **已后置**隧道共同应用；物理出口/严格路由要真实能力 |
| A13 | DNS上游CRUD、分流、三回退链、Fake-IP/JSON及dns-settings测试 | 删除默认服务器自动改选首项，缺明确引用迁移确认 | 平台DNS与内核共同生效未验收 |
| A14 | 设置迁移白名单/schema/引用/预览/CAS、取消和未知状态测试 | 未发现应重复实施的迁移链；范围限制继续显示 | 系统保存/分享及设备提供者验收 |
| A15 | 端点catalog/control/details、方向协调、来源恢复、Peer事实及network-endpoints测试 | 已有客户端链，不能据此称所有协议/设备完成 | 原生设备/Peer/协议真机场景 |
| A16 | registry、独立plane、u64、Host只读、共用TrafficWorkspace及测试 | 已有客户端链，不另造统计数值 | 手机采样/重连、Host实际后端 |
| A17 | 固定选择、capability/permission/epoch、未知核对与端点来源检查 | 已有客户端链 | 手机资源清空及活动连接不变验收 |
| A18 | 连接筛选/暂停/详情/历史/导出，Packet真分页与原操作屏障 | **Flow缺多选关闭和逐项结果，fulfilled却closed:false也被计成功；**需宿主固定身份/目标执行链。Packet不重复加固 | **已后置**移动owner/事件恢复；真实手机交付 |
| A19 | 固定脱敏日志/诊断、有界捕获/任务scope及mobile日志诊断测试 | **路由UI错误显示逐跳TTL/RTT，实际core为规则评估；dns_cache无独立入口。**可复用jobs完整修复 | 真实运行采样未接时unsupported；不造数据 |
| A20 | 桌面签名安装/本地信任/版本权限审阅及plugins测试 | 移动安装链未开放，仍是客户端平台实施工作 | 容器/分发待决定及验收；不是全部core阻塞或整体已后置 |
| A21 | 桌面逐组件配置/授权、管理页及有界SDK通道和测试 | 移动SDK IO/非JIT/敏感加载/来源通知未完成 | 具体平台容器与允许路径；未宣称设备运行 |
| A22 | owner清理、plugin_cleanup及归属/重试测试 | 卸载确认缺具体资源、替代配置选择与逐项剩余结果 | 移动插件执行尚未开放 |
| A23 | 两入口、真实有界目录/说明/渠道映射/偏好及mobile版本测试 | 自动检查只有intent，可补前台due实际检查；安装器/OS调度仍待客户端平台实施 | **已后置**runtime/bundle装配；签名/商店发行未授权 |
| A24 | 产品/作者/标识/实际客户端版本/链接/许可原文搜索及About测试 | 四项许可资料缺口继续保留，不能用他包版权补齐 | 真实运行/随包Zero版本依赖尚未接入的产物 |

## 其它独立用户能力

本地数据已有按范围确认清理及捕获写入协调，但容量、保留预算和逐项失败核对UI仍可独立补充。
普通历史/未知事务的自动重放、泛化清账不在候选功能中。旧无回执记录仍只读，恶意全量回滚仍不作保证。

## 本批选择与排除理由

优先补A08普通配置的无损文本编辑链。它直接影响用户添加/编辑材料，而非继续扩大恢复机制。
不将本批称作A08全部完成：任意外部材料授权、订阅来源/覆盖编辑和真实运行启用仍有各自边界。
已冻结为普通内嵌Zero来源：保留来源Value中的有效字段及u64，不经JS JSON往返；原生schema/引用校验前阻止未授权规则集/GeoIP读取。保存结果为待应用，未知只读核对原固定对象、不自动重放。准确测试结果在同PR最终提交与验证记录中维护。

冻结材料组件已审计：现`workspace::Prepared`已经保存完整source/effective/binding；普通spawn仍
重读当前材料，Restart在现命令明确禁止。当前没有可消费封存文件并确认加载的生产链，新增秘密
落盘只会增加副本。Windows创建时DACL、typed依赖、相对目录、可变quota/outbox/log输出、原
child与所有启动/watchdog屏障必须作为完整链实现，不能由一个临时文件工具代替。
无落盘typed依赖准入可独立复用，但它不解除Restart。完整证据与最小实际Proxy启动ack要求见
[冻结启动边界](frozen-startup-boundary-v0.0.3.md)。

## 最小协调清单

- CORE-01：外部平台TUN/packetFlow接入；CORE-02：外层socket protect/bind和网络代次。
- CORE-05：固定core的非桌面TUN返回类型编译；CORE-03：确需通用wire写操作的原子前置条件。
- 冻结启动：实际Proxy加载材料/依赖与初始化事实的绑定回执，不接受客户端摘要回显或PID/health替代。
- 客户端仍拥有安全材料通道、依赖封存、原child/恢复和watchdog协调，不把这些统称core阻塞。
- 平台/设备：允许的iOS宿主与entitlement环境、Galaxy S8/API24及真实iPhone验收；不购买账号或创建凭据。

证据索引：[验收账本](mobile-acceptance-ledger-v0.0.3.md)、[todo](../../todo.md)、
[上一批验证](../verification/mobile-client-v0.0.3.md)。本矩阵后续按实际代码增量更新，不用成功的
静态页、浏览器fixture或局部组件编译替代正式验收。
