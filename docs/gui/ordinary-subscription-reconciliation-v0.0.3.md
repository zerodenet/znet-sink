# 普通订阅未确认结果的只读核对

`subscription_reconcile_unknown` 是宿主主窗口的只读入口，绑定普通订阅 ID 和十进制字符串 `expectedUpdatedAtUnixMs`。它不发新的订阅 HTTP 请求，不提交配置、不重启内核、不清条件记录或 `lastError`。托管来源通过插件来源管理，不能借此查看托管材料。

返回的状态固定为 `result-unknown`，`canResolve: false`。本地事实包含来源版本、最近成功检查时间，以及关联材料的存在、普通归属、活动标志和 Zero 原生 schema 检查结果。`saved-valid` 只表示当前已保存 JSON 通过原生结构解析；不证明协议编译、规则文件仍有效、原更新候选相同或当前转发生效。返回不包含来源 URL、UA、配置正文、凭据、条件头或原始错误。

## 实际事务顺序与缺少的证据

当前普通 200 同步按以下顺序执行：

1. 共享 HTTP 有界读取、格式解码/解析；准备规则集产物和本地入口、策略选择。
2. 构建候选 profile，仅在内存中保存本次源正文/转换结果；有效配置再组合全局与 profile 覆盖。
3. 不需运行变更或内核离线时，先发布 profile 关系库、应用设置投影，再处理受支持的桌面系统代理归属。移动端这类保存仍是本地材料/待应用，不是接管。
4. 运行中活动配置变更先 validate，再经 `configuration::apply` / `capture_apply` 提交 `config.apply_runtime`。当次确认要求 runtime-only 回执、实例相同、回执 revision 与后读身份相同；需要时核对 TUN 停建。`capture_apply::Receipt` 保存本次前后配置/计划和已确认身份，仅存在内存。
5. 当次运行确认后才发布本地 profile；本地发布或系统代理失败时，尝试基于当次 Receipt 协调恢复，不能确认恢复则返回未确认。
6. profile 返回后单独更新订阅关联、lastSync/元数据并清上次错误，最后保存已确认材料绑定的条件 HTTP 记录。

源下载、规则产物、本地关系库、应用设置/系统代理、运行提交及条件记录不组成一个跨进程原子事务。候选可能尚未发布，已保存材料可能是旧材料或新材料；`lastError` 只有宿主固定未确认原因，没有持久候选、操作 ID、原实例/版本或完整回执。目前 schema 因此只能报告 `candidate/receipt: not-recorded`，不能从“当前 profile 存在”推断本次候选已应用。

## 桌面与移动运行读取

桌面通过同一个 `BoundControl` 实际查询两次 typed `runtime` 身份，经过共享宿主只读能力 lease（一次操作、2048 字节结果预算、5 秒截止）；原生 IO 由已有 IPC 传输承担。实例或配置 revision 变化返回 `changed`，权限拒绝/断联/字段缺失返回固定原因的 `unavailable`，没有字段猜测或 endpoint 中途重发现。

只有前后身份一致才返回 `readable` 的实例及无损十进制 revision；这表示当前身份稳定可读，仍不证明原候选生效。`ConfigSnapshot` 是 mode、规则数量、监听器、公开出站和组的投影，不包含全部 DNS、规则内容和秘密，因此本入口不查询或比较投影来宣布完整配置相同。

移动运行桥未接入时返回运行 `unsupported` 和明确原因，仍可展示本地来源与材料事实。移动分支不进入 IPC，也不使用桌面默认 socket。普通 per-ID claim 和 profile-operation 锁避免与本客户端同步/修改交错；返回前再次比较来源、材料及当前配置的原生读取范围，迟到变化拒绝旧结果。它们不能替代其他控制端的远端原子 CAS。

## 后续可独立实现的有限恢复

客户端可为今后普通事务增加有界持久记录：绑定来源/材料身份、组合版本和目标实例，发送前记阶段；收到并核对真实回执后持久保存 confirmed 身份及恢复范围，记录本地关系发布和订阅完成阶段。对“已持久 confirmed、仅本地完成中断”的事务，可以再次固定实例/版本和本地材料核对后，仅完成相应本地提交，不重发 HTTP/apply。

普通候选若需持久保存，应采用普通材料的可信关系存储及明确容量/清理规则；现有普通 capability byte store 每值 64 KiB，不能拿它塞大配置或受保护明文。托管/受保护来源不能降级进入普通候选表。

丢失 submit 回执、confirmed 回执持久化前崩溃、历史固定文字错误、实例/版本变化，仍没有足以安全解除的证据。完整此类核对需要已有 owning runtime 能权威查询操作结果或稳定候选摘要及权限、版本绑定；这是通用控制契约协调项，不在本入口写 core、装配原生服务或清错误来绕过。原生 VPN/core/后台服务继续后置。

## 验证边界

服务测试验证本地材料状态区分、源版本前置条件、同毫秒迟到变化、受保护归属不读正文、核对后 source/material/error 无变化、移动 unsupported 不进入能力/IPC。真实 Unix socket fixture 走生产 BoundControl/query/parser，覆盖两次相同身份、实例/版本变动、缺 revision、权限拒绝和断联；大于 JavaScript 安全整数的 revision 保持字符串，server/配置秘密 canary 不进入 DTO 或能力记录。薄命令另测主窗口限定和 canonical u64 decimal。

仓库内可重复执行的前端回执测试：`pnpm exec node --test scripts/test-subscription-reconciliation.mjs`（16 项）。测试通过 TypeScript `transpileModule` 执行完整生产 `config.ts`，仅替换 Tauri invoke/状态信号/平台 IO ports，不复制校验实现；覆盖不安全来源版本在 invoke 前拒绝、固定 id/version/关联材料、虚假完成或回执拒绝、无损 u64/null 事实，以及所有核对分支都没有 profile/rule signal、配置激活、观测重启或 TUN mutation。脚本并入 `pnpm test:mobile` 后可随前端验证统一运行。

这是实际客户端传输和状态测试，不是 pinned Zero 二进制的完整网络运行测试，不是手机真机验收，也未证明丢失回执的历史操作已生效。
