# 单端点客户端来源恢复

此实现补充 v0.0.3 A15/C08 的普通配置来源恢复路径。它不等于整个 A15 或移动 VPN 真机验收完成，也不新增 Zero 公共 API。插件托管、受保护或没有可信来源回执的端点保留解释性不可用状态。

## 来源身份与预检

`gui_endpoint_catalog` 在现有客户端配置操作锁内调用 `endpoint_restoration::remember_catalog`。来源必须是活动普通 profile，源声明必须唯一匹配 tag 与 protocol，端点必须声明 `configuration.origin=canonical`。这些条件只提供基础归属：真正准入还需要已有工作区 `ConfigTransactionReceipt`，其 profile、有效配置摘要和实际运行 `core_instance_id/config_revision` 与当前 composition 和内核一致。只凭相同 tag、版本号、成功 Toast 或当前页面显示不授予来源写资格。

绑定缓存保存固定 opaque endpoint ID、profile、tag/protocol、源内容摘要与版本、确认配置摘要和白名单公开资源身份，并固定 AppConfig 与规则集文件各自的 `HistoryToken`。绑定 ID 包含这两个 token；文件改成 B 再改回 A，旧 ID 不能借同值的新 catalog 绑定恢复权限。预检不接受客户端自行构造的源标签。

确认计划有效期五分钟。私有记录另固定宿主 `hostSessionId` 与客户端 `scopeGeneration`；提交前核对当前宿主、代次、`ClientScope`、期限、完整 AppConfig、源版本及最终合成摘要。即使新宿主的本地计数相等，或当前配置内容恢复原值，旧预检也不能继续提交。配置、实例、意图、generation 或来源变化需重新预检。

离线缓存只来自之前确认的普通来源绑定。新建的离线计划可保存本地覆盖移除，但 `runtimeAvailable=false`，不能进行运行应用。预检打开以后客户端 scope 变化，包括本地保存路径，也会拒绝旧确认。未核对事务阻止创建重复目标计划。

## 复用链与单目标保存

`configuration::endpoints::restore_candidate` 删除 `AppConfig.profile_edits[profile].endpoints[tag]` 中协议匹配的单项覆盖，并清理空容器；来源 JSON、其他端点、DNS、全局偏好及其他 profile 字段保留。不会写回订阅源。

`rule_overlay::compose_effective_candidate_with_app` 将该候选 AppConfig 传入已有 composition 及规则覆盖服务，保留原生 schema 和实际优先级。它不重建端点配置方言、不借导出临时配置绕过 runtime-only 路径。

本地发布通过 `endpoint_restoration::history` 核对两个文件的原 token、实际 typed AppConfig/规则集与内存值，再由 `app_config_store::save_owned_locked` 以原 `plan_id` 作为 owner 写入。同步发布持有规则数据锁、客户端 scope fence、AppConfig 锁及 AppConfig→规则集文件历史锁；不持有这些文件锁等待网络。客户端 scope 在整个本地发布段不能切换。成功回执包含真实文件写入的 before/after token 和原 owner，确认后才发布内存结果。

文件历史 token 包含随机 store 身份、文件身份和持久递增 epoch。正式 writer 的同值写入也推进历史，因此摘要相同不能代替 token；AppConfig 的主文件和备份同时纳入原写入事实。离线保存仍为 `saved_pending`，只确认本地保存，不确认 VPN、内核配置或端点恢复。离线待应用材料重新绑定 catalog 时，还须证明当前 token 对应原操作的 owned 写入回执，不能只重建出相同配置摘要。

## 在线事务与不确定结果

在线执行先核对原资源与 generation，校验最终原生配置，再按顺序执行：

1. 持久记录 `applying`，调用既有 `configuration::apply::apply_with_identity` runtime-only 配置链。
2. 保存实际应用回执 `core_instance_id/config_revision`，再次读取同一 opaque 端点及其角色、协议、canonical 来源和新 revision。
3. 保存 `clearing` 阶段，调用既有 `network_endpoint::control_at_revision` / `clear_overrides`，使用新意图版本与实际实例。
4. 核对端点返回状态与来源配置意图，并重查文件历史。只有实际运行配置与端点确认后，才持久记录 `local_write_started`，随后尝试一次原 owner 的单项客户端覆盖保存。该标记只表示原写入可能开始，不表示已经写入成功。
5. 最终 `confirmed` 同时要求 localSaved、runtimeConfirmed、实际 applyIdentity 与端点快照；写入现有工作区确认回执。

Journal 位于应用私有数据目录 `endpoint-restoration-v1.json`，最多 128 个来源绑定、32 个事务、1 MB，严格 schema 与原子写入。保存公开身份、摘要、宿主/文件历史绑定、原 owned 写入回执及协议绑定的 enabled/directions 覆盖，不保存代理正文、密钥、订阅 URL/header/token 或完整 AppConfig。损坏、未知版本及存储失败不会降级为空成功状态。旧 journal 缺少新增历史或 owner 字段时仍可读取原回执，但只保留历史；不会补发新 token 来恢复旧操作的执行资格。

提交无回执、断联、资源替换、运行成功但本地 CAS 失败分别保留阶段与 `result_unknown`；不会宣布回滚。相同 plan 再次提交只返回已有回执，不重放。缺失配置应用回执不能由“当前看似相同”的字段推断补齐；尚未进入 clear 阶段的事务不会在核对中自动补发 clear。核对只读取内核原操作的身份与来源状态，仍要求当前源、原客户端 scope 和已确认应用 revision。各网络阶段前后核对文件历史；变化后不继续清除端点或保存旧候选，也不把已发生的运行变更声称为回滚。

如果原操作已经持久记录 `local_write_started`，核对不再写一次 AppConfig。它只接受实际 typed 文件恰为原候选，且文件历史的 pending intent 或 clean owned receipt 同时匹配原 `plan_id`、before token 与实际主文件/备份。`confirm_owned` 只补齐文件历史元数据，再发布已经存在的本地值；不会重写正文、下载配置或重放 apply/clear。后来其他操作写出的相同值不能替代原 owned 回执。离线本地保存也使用这一固定 owner 核对路径。

若运行结果已确认、原记录证明从未开始本地写入，只有仍属于当前宿主/代次且原文件 token 未变化时，用户明确核对才可开始一次本地保存。若原写入可能已开始但缺少可证明的 intent/receipt、正文仍旧或已变化，则保留未知并阻止重写；可读文件或缺少可见覆盖不构成完成证据。

前端恢复当前源、版本、profile 和客户端实例的持久事务。即使配置应用失去回执并推进运行 revision，原未知事务仍可显示，不依赖一个已失效的新 catalog binding。收到不完整 `confirmed` 字段组合仍显示结果未确认。关闭页面不取消宿主事务；配置变化使旧预检失效。四个命令均仅允许主宿主窗口并遵循现有管理权限；插件页面不能读取该 journal 或调用恢复链。

## 当前 core 并发边界

客户端锁协调本应用的配置写入；IPC 采用现有私有实例边界。`clear_overrides` 的 core/intent 前置条件由 Zero 内核在共享 apply 锁下校验。读取过期 intent 的命令不能控制删除后重建的同 ID 逻辑绑定：内核意图 revision 分配器跨配置快照共享且单调变化。客户端观察到 generation 变化时会拒绝旧预检。

现有 core `config.apply_runtime` 不提供 expected configuration revision 的原子 CAS；端点 clear 也没有 expected config revision 或 physical generation 的原子命令前置条件。物理设备 incarnation 可以变化而逻辑意图不变。不能通过猜测参数补足这些接口，不能声称在任意外部写入者或独立设备重建竞争下具备原子安全。外部写入者能在预读与命令之间改变同逻辑绑定的私有协议材料，这仍是跨仓库接口缺口。这里保留严格回执与前后读 guard；需要未来通用 core CAS 才能扩展该保证，本任务未修改 core。

文件历史协调正式 writer，并识别检查时实际观察到的外部变更；它不能重建客户端退出期间未被观察的外部 B→A 历史，也不防拥有私有目录权限的本地恶意回滚。文件 token 与客户端 scope fence 不等于远程 core 的原子 CAS。

损坏存储的只读诊断及“不归档为空账本、不清未知屏障”的用户边界见[恢复存储只读检查](./recovery-storage-inspection-v0.0.3.md)。这类诊断不产生端点恢复或文件重写权限。

## 验证与交付限制

测试覆盖单项移除与其他设置保留、canonical 来源资格、同标签不同协议、duplicate source、apply→新意图 clear→本地发布顺序、丢失 apply/clear 回执、资源替换/generation 变化、本地 CAS 冲突、离线保存无内核调用、journal 重建/损坏、scope 与过期拒绝、宿主窗口权限。

文件历史专项使用临时目录中的正式 AppConfig/规则 writer，验证主文件与备份都回到原始字节的 ABA、规则文件 ABA、同值写入、固定 owned receipt、实际写入后丢回执的 metadata-only 确认、其他 owner/后来同值拒绝、并发同票据仅一次保存、旧宿主与代次拒绝、旧 schema 记录只读以及本地 IO 失败。执行结果以统一验证记录为准，不把源码中的测试数量视作通过。

`node --experimental-strip-types --loader ./scripts/lib-alias-loader.mjs --test scripts/test-endpoint-restoration.mjs scripts/test-network-endpoints.mjs` 验证固定计划、离线待应用、失效确认零写入、失去回执后只核对、不完整 confirmed 不假成功，以及已有端点契约。`pnpm check` 验证 Svelte/TS。Rust 执行结果由同一工作区完整检查记录报告。

这些测试的 Fake backend 仅用于验证事务控制与失败契约，不作为真实流量、平台 VPN 或服务恢复结果。临时目录的文件失败测试不等于手机存储故障验收；尚未执行 Android/iOS 真机来源恢复、锁屏与网络切换、真机磁盘故障或真实端点协议场景；Android 原生 VPN 数据面与 iPhone 签名/真机能力仍按主交付报告的限制记录。
