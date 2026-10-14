# 普通订阅未确认结果的核对与有限本地恢复

宿主主窗口提供两个独立动作：

- `subscription_reconcile_unknown(id, expectedUpdatedAtUnixMs)` 只读核对来源、已保存材料、持久候选/回执和当前运行身份，不修改来源、错误或内核。
- `subscription_recover_confirmed_local(id, expectedUpdatedAtUnixMs, recoveryId)` 只在核对提供可恢复的原记录后，由用户明确完成该记录的本地发布。它不下载订阅、不重新提交 apply，不重启内核或操作 TUN。

版本参数使用 canonical 十进制 u64 字符串。`recoveryId` 固定本次候选及阶段，不能以当前同名来源或当前 revision 替换。托管、受保护材料仍通过插件来源管理，不进入普通恢复表、普通编辑器或本入口。

## 核对结果与权威证据

没有记录或没有持久确认回执时，结果保持 `result-unknown`、`canResolve: false`。`evidence.candidate` 区分 `not-recorded/recorded`，`evidence.receipt` 区分 `not-recorded/confirmed`；有候选不等于已生效。只有原 confirmed 回执、候选及本地范围合法，且当前实例/revision 符合原身份时，才提供 `local-publication-pending`、`canResolve: true` 和固定 `recoveryId`。

本地事实仍区分材料缺失、未关联、归属不符及 `saved-valid/saved-invalid`。`native-schema-only` 只表示当前已保存 JSON 通过 Zero 原生结构解析，不证明协议编译、规则文件仍有效、它等于原更新候选或当前外网转发健康。UI 回执不包含 URL、UA、配置正文、凭据、条件头、AppConfig 正文或原始错误。

## 本批可恢复范围

生产 ledger 路径限普通 Zero 来源、活动配置的真实运行变更，以及本地完成不需要重新调整外部 OS 系统代理的事务。没有 runtime 变更或内核离线的保存仍使用既有本地链路，不从它们生成 confirmed 运行证据。

桌面托管系统代理已启用时，客户端比较当前与候选的有效 host/port/native bypass，并核对实际系统状态新鲜度与 bypass 一致性。若需要 retarget 或不能确认上述条件，继续使用原 `upsert_runtime_locked` 的真实 retarget/补偿链；不为它提供本批 local-only 恢复，也不跳过系统代理以获得假成功。该能力仍保留，不能把其失败称为可只写 SQLite/AppConfig 修复。

## 普通事务的持久阶段

账本属于 `app_database::ordinary_recovery`，业务编排属于 `ordinary_subscription_recovery`。真实 HTTP 读取、格式转换及规则准备继续使用既有服务；账本自身不执行网络或 runtime 命令。

| 阶段 | 已持久事实 | 可执行范围 |
| --- | --- | --- |
| `prepared` | 普通候选、来源/材料/关系/AppConfig/独立 typed 规则集/有效配置及接管计划摘要；来源固定未确认屏障 | 只读检查；没有确认回执，不能发布候选或重放 |
| `apply-pending` | 原记录标明即将进入运行提交 | 缺回执仍未知；不能换操作重新 apply |
| `runtime-confirmed` | 实际 runtime-only 回执、原实例/revision、候选与接管计划核对事实 | 当前范围和身份再次满足后，可完成本地发布 |
| `local-committed` | SQLite 已提交候选 profile、关联关系及阶段摘要；来源仍保留原屏障和上次成功检查事实 | 核对 AppConfig 投影并继续本地完成，不能新同步 |
| `complete` | 本地投影确认后，在同一 SQLite 完成事务中发布最终来源、移除屏障并保存完成摘要 | 仅精确匹配的已完成记录可作为重复调用回执；不再发布或改时间 |

`configuration::capture_apply::Receipt` 本体仍只在当次流程中持有；账本保存已验证候选/计划摘要与 confirmed 身份，不保存可重新执行的 Receipt、旧明文运行配置或任意 IPC 请求。只有实际回执已被核对并成功持久化，才进入 `runtime-confirmed`。回执丢失，或收到回执后但落盘前进程中断，不能从当前身份可读推断原提交成功。

准备阶段把候选和来源屏障写入同一 SQLite 事务，并冻结屏障版本及最终来源版本；最终版本不早于屏障，溢出拒绝，恢复和重复恢复不临时重算时间。`lastSyncAtUnixMs` 保留原真实 HTTP 成功检查事实；来源 `updatedAtUnixMs` 是持久记录的发布版本，两者不混用。来源、材料、关联、客户端覆盖或组合版本变化须重新核对，不能忽略同毫秒的语义编辑，也不能让重复屏障错误写入制造一份新的候选。

## 本地发布、重启与重复恢复

SQLite 关系/账本和 `app-config.json` 仍是两个持久边界，不构成跨文件或远端原子事务。SQLite 先写 `local-committed`，AppConfig 投影确认后再进入 `complete`；中间失败保留来源屏障及可追溯记录，不先清 `lastError` 宣称完整成功。内存与前端状态只在实际对应持久阶段成功后发布。

恢复使用共享 per-ID claim 与 profile-operation 锁，重新核对原来源、材料、关系图、AppConfig、独立 typed 规则集及运行实例/revision。规则集属于独立 store，准备时冻结其摘要，实际回执确认、各恢复阶段及最终完成均重新核对；不把其他托管规则正文复制进账本。最终 SQLite 完成时同时持有关系、AppConfig 与规则集锁，保持本地前置条件到 CAS 提交。只接受记录对应的前后阶段，不覆盖用户随后改变的关系、活动配置、设置或同名替换对象。重复调用只在完成摘要和当前最终关系/来源仍精确匹配时返回原完成事实；它不刷新时间、不再次写 profile、下载或 apply。完成摘要被清理后，旧 ID 不成为新的写入许可。

启动 loader 分别读取关系库与 AppConfig，并可能执行既有 DNS/TUN/client-edits 迁移或备份恢复；不能把重启后的内存视为原进程的旧快照。准备前、屏障、已发布本地及最终关系图分别留存摘要；只在数据库是该记录的权威阶段、内存仍精确匹配其中一个已知阶段时，才提升内存缓存。核对过程中发生此提升可要求一次刷新，不把旧内存写回数据库。混合阶段需要依据账本及实际投影核对，无法证明等价的规范化、备份恢复或新设置漂移应拒绝旧候选。设备或内核重启导致实例变化时，即使本地候选还在，也不自动重放原 apply。

本动作完成的是普通材料与客户端投影，不宣称当前 VPN 接管、DNS/物理网络出口、端点健康或桌面系统代理仍处于原状态。SQLite、文件投影和远端运行查询之间的检查也不能替代其他控制端的远端原子 CAS。

## 桌面运行读取与移动限制

桌面继续通过同一个 `BoundControl` 实际读取两次 typed runtime 身份，经过共享宿主只读 lease（一项操作、2048 字节结果预算、5 秒截止）。实例或配置 revision 变化返回 `changed`，权限拒绝、断联或字段缺失返回固定原因的 `unavailable`，不猜字段或中途重发现 endpoint。身份及 revision 采用无损字符串 DTO。

`ConfigSnapshot` 是公开配置投影，缺少完整 DNS、规则与秘密；不拿它比较来证明原完整候选相同。新增恢复证据来自已持久 confirmed 记录，当前身份只用于检查这份原证据仍适用。

移动 owning runtime 尚未接入时，运行读取返回 `unsupported`，不进入桌面默认 IPC，不提供本地恢复准入。只读的本地材料事实仍可展示。没有记录的历史固定文字错误、prepared/apply-pending、丢失确认回执、实例/revision 或组合变化仍须保持未知；其解除需可关联原操作的权威结果或允许读取的稳定运行摘要，不能清缓存或再下载来绕过。原生 VPN、完整 core 装配与后台服务继续后置。

## 条件缓存与验证边界

本批完成收据不包含原 HTTP validator 的恢复。未持久保存 ETag/Last-Modified 时，本地恢复仍不补造条件记录，也不为完成此动作重新下载、清记录或提交配置；后续独立用户/到期同步按现有来源/材料/版本绑定判断是否能发送条件请求。详见[条件同步边界](./ordinary-subscription-conditional-sync-v0.0.3.md)与[存储容量和阶段](./storage.md)。

回归应分别检查准备/确认落盘、SQLite/AppConfig 投影中断、重开后的屏障、自己的错误更新与同毫秒来源/独立规则集编辑、重复完成、版本/实例变化和有限容量；真实 Unix socket fixture应覆盖 pinned runtime 查询及原 ID/range 固定，确保恢复没有 HTTP/apply/TUN 副作用。前端测试执行生产 `config.ts` 的回执校验和固定确认范围，不能只证明静态按钮存在。

实际运行的测试及未运行项记录在[统一验证记录](../verification/mobile-client-v0.0.3.md)。本文不把新增源代码、局部测试或本地发布成功称为完整移动网络、真实手机后台或原生安装验收。
