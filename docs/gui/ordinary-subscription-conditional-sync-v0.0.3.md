# 普通订阅的条件 HTTP 同步

普通 URL 订阅复用 `znet-client-capabilities::network::get_conditional`，不新增网络执行器或 UI 队列。手动同步、桌面自动同步、移动原生前台到期协调共用同一 per-ID claim；启停自动参与不会禁止明确的手动同步。托管订阅继续由插件来源处理，不共享普通订阅的 validator。

## 200 和 304 的确认边界

200 必须是有效 UTF-8，并经既有格式解码、原生配置转换、路由与规则依赖准备、材料保存和受支持的运行应用核对后，才保存 ETag/Last-Modified。材料发布与条件记录保存不是一个跨域原子事务：后者失败会返回独立 `subscription_cache_error`，保留实际已保存材料及最近成功同步记录，不宣称完整成功或回滚；下一次不能使用失配记录掩盖这个阶段。

私有记录包含版本、原始来源身份摘要（订阅 ID、URL、有效 UA、格式、kernel）、关联配置身份摘要、实际发布的材料 SHA-256 和 revision、最近成功同步 revision/time，以及每项最多 4096 字节的 validator。没有原始 URL/UA/body，validator 不进入 UI、通用日志或诊断；来源信息与其可能含有 token 的 validator 只存在应用私有 capability namespace。SQLite 使用 revision CAS；客户端存储额度仍是每 owner 1 MiB/256 项、全局 64 MiB/16384 项。额度/磁盘不足是明确的记录保存阶段失败。

请求前重新检查关联材料存在、普通归属、原生 Zero schema、内容与 revision，来源身份与成功记录相同，且无上次错误；只在全部满足且有 validator 时发送条件字段。修改 URL、UA、格式、kernel、关联材料/目标、源版本或上次同步结果，会使旧条件记录不再可执行。下一次有效普通检查使用无条件请求，不删除旧有效材料；无条件请求收到 304 则明确拒绝，不把空 body 当配置。

有条件的 304 返回后，再持有共享 profile-operation 锁并核对完整订阅快照和材料身份。确认持久成功才发布检查时间，并更新相同材料的条件记录。此路径不下载正文、不发布新配置、不重置客户端覆盖、策略选择或活动配置，不提交 runtime apply，也不解除此前未知的运行结果。保存故障/迟到编辑/材料损坏均保留旧确认值；不会自动再发一次无条件 GET 来伪造 304 成功。

`lastSyncAtUnixMs` 是成功的来源内容检查时间，不是流量采样时间。304 携带真实 `subscription-userinfo` 时，仅更新明确提供且有效的字段；缺失字段保留旧来源值，不造零。没有独立额度观测时间时不得把新的同步检查时间解释为额度新鲜度。过大的到期秒数无法无损转换为毫秒时保留未知。

## 安全与剩余平台边界

网络 lease 沿用 30 秒、8 MiB 正文、10 次重定向及共享网络访问策略/取消边界。错误只包含固定阶段原因或 HTTP 状态；正文、URI token、请求/条件头和内核回执里的秘密不回填 Toast、订阅 `lastError` 或操作记录。格式错误不创建半有效配置，下载失败与运行/存储提交失败分别表达。

移动端保存的新材料仍可是待应用；本功能不启动 VPN、不提供内核运行所有权，不把离线保存称为接管。系统后台调度尚未适配，原生前台 pass 是实际生命周期协调；无定点/锁屏执行保证。Android/iOS 真机网络、后台中断与存储耗尽尚未运行。跨材料、运行和条件记录的崩溃窗口以保守失配重新检查，不承诺原子事务。

## 验证

`cargo test --manifest-path src-tauri/Cargo.toml --lib subscription::original --locked` 包含实际 localhost 200→解析→私有 SQLite 保存/重开→携带 ETag 与 Last-Modified 的 304，以及源编辑后的无条件 GET。其他检查覆盖不参与自动同步仍可明确请求、意外 304、非法 UTF-8/503、缓存损坏/头注入/长度预算、SQLite CAS/namespace 隔离、源/材料/版本/未知结果失配、持久失败前不发布、迟到选择变更不覆盖，304 无材料/配置操作副作用及敏感 canary 不出现在错误和 capability 操作记录。

这些是共享服务与真实本机 HTTP/SQLite 证据，不等同于手机端真机后台验收或内核转发验收。

## 未确认屏障与有限本地恢复

普通运行应用的新持久候选/回执与本地阶段记录位于 schema 3 的独立可信关系账本。prepared/apply-pending 或缺 confirmed 回执仍是未知，不能因当前 runtime 身份可读、改名、关闭自动参与或清 `lastError` 重放。已确认应用但本地发布未完成时，可经 `subscription_reconcile_unknown` 核对原记录，再由用户明确执行 `subscription_recover_confirmed_local`；该动作只完成原本地候选，不发 HTTP、apply 或 TUN 操作。ledger 准入限活动普通 Zero 配置的真实运行变更且不需外部 OS 系统代理调整；需要 retarget 或实际系统代理状态无法确认时，仍走既有真实 retarget/补偿链，此类事务不属于本批 local-only 恢复。`local-committed` 保留来源屏障直到 AppConfig 投影及 SQLite 完成阶段确认，不能让自动或手动同步抢先继续。

条件记录不属于本批本地恢复的成功收据。原响应的 ETag/Last-Modified 若尚未持久保存，恢复不编造或宣称它们已恢复；也不为完成此动作再次下载、删除缓存或清空错误。下一次独立的用户/到期同步照常检查原来源、关联材料、source/material revision、lastSync 与 lastError：已有 validator 全部匹配才可条件 GET；缺失或失配时按普通流程无条件 GET。这个后续检查是新的来源同步，不是原未知操作的重放或恢复阶段。

没有持久 confirmed 回执的历史操作、确认落盘前崩溃、当前实例/版本、AppConfig 或独立 typed 规则集组合改变，仍没有安全解除证据。完整解除需可关联原操作的权威结果或稳定运行摘要；不能通过手动下载、删除错误或清日志冒充。接口与存储限制见[普通订阅核对与有限恢复](./ordinary-subscription-reconciliation-v0.0.3.md)。

损坏条件记录的修正使用显式宿主操作 `reset_conditional_cache(id, expectedUpdatedAtUnixMs)`，返回 `{ cleared }`。它取得同一 per-ID claim，拒绝托管来源，核对当前来源版本，然后读取私有原始记录的 SQLite revision 并执行 CAS Delete；不要求能解码损坏记录，不盲删、不请求网络、不提交运行配置。仅删除当前普通订阅的 validator 行，源、配置材料、客户端覆盖、lastSync、lastError 和固定未知屏障均保持原值。空记录返回 `cleared: false`；来源/行版本变化要求重新确认。清除条件记录可让下一次已获准的普通同步重新获取正文，但不是未知运行结果的解除操作。
