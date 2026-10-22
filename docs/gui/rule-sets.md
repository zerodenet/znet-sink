# 规则集配置

规则集页管理 GUI 创建或导入的公共资源；节点订阅随附的提供者属于订阅内部资源，
不通过普通规则集列表、详情或迁移入口取得编辑权限。内置资源保留启停和绑定，不能普通删除。

`RuleSetProfile.id` 是 GUI 主键；原生公共 tag 使用现有 `gui-common-<hash(id)>` 映射。
二者、来源名称与 artifact 路径不能互相冒充资源身份。原生来源自己声明的规则集继续按来源归属管理。

## 读取及维护

`rule_set_list` 返回 `RuleSetSummary[]`，避免将大型远程规则数组传给页面；
`rule_set_get({id})` 返回适合详情/本地编辑的完整资源。可编辑语义是实际 Zero Rule IR，
通过 `semanticIr` 输入；普通来源元资料为 `url/format/updateIntervalSecs/userAgent`。
当前模型还包含 `commonBinding`、`sourceState`、`artifact`、更新时间及可选同步错误。
旧版 `format + source.kind/content` 示例不再代表当前 DTO。

新增、更新和同步仍使用现有 `rule_set_upsert/update/update_all`、内置更新及公共绑定命令。
公共绑定的启用、action 与 order 属于资源自身；当前模式、客户端偏好、编译产物及实际运行校验
共同决定是否注入，不由页面中的开关单独证明运行已生效。

## 删除及引用迁移

删除改用[规则集引用与 DNS 迁移契约](reference-migrations-v0.0.3.md)。
旧 `rule_set_remove({id})` 保留明确拒绝回执 `rule_migration_preview_required`，
不再先删除本地记录再自动协调运行态。

| 命令 | 输入 | 行为 |
| --- | --- | --- |
| `gui_rule_migration_snapshot` | `profileId, ruleSetId` | 当前资源身份、原生 tag、真实引用位置、受限范围和声明替代项 |
| `gui_rule_migration_preview` | `input:{profileId,ruleSetId,binding,targetId}` | 固定引用、替代资源、删除影响及候选；未引用资源可不选替代 |
| `gui_rule_migration_save` | `planId` | CAS 核对后先发布引用、再删除旧记录，保存待应用 |
| `gui_rule_migration_status` | `planId` | 只读核对原操作及本地阶段，不重放 |
| `gui_rule_migration_discard` | `planId` | 释放尚可丢弃的预览；未知原操作保留屏障 |

托管/受保护/无法检查的材料及不能安全迁移的跨配置引用明确阻止提交，不静默遗漏引用。
引用保存在客户端覆盖，原始来源和关系不改写；未知字段与 u64 留在 Rust Value 中。
产物文件保留，删除记录不结束已有连接。跨文件阶段失败不宣称原子回滚，
本地 `saved_pending_apply` 也不代表内核运行、DNS 或 VPN 已应用。
