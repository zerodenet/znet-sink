# 本地存储边界

GUI 的本地持久化按数据性质拆分，避免把所有内容塞进单个配置文件，也避免为简单启动配置引入不必要的数据库依赖。

## SQLite 管理的动态业务数据

应用数据目录下的 `znet-sink.db` 集中管理：

- 代理配置记录、顺序和唯一 active 状态；
- 订阅记录、订阅地址、启用状态；
- 订阅到代理配置的 `targetProxyConfigId` 外键映射。

代理配置和订阅的关联变更在同一个即时事务中提交。删除代理配置时，引用它的订阅映射会同步清空，避免内存状态和持久化状态分叉。

## 继续使用文件的数据

| 数据 | 存储 | 原因 |
| --- | --- | --- |
| 应用启动与 UI 简单配置 | `app-config.json` | 可读、可恢复，启动阶段无需先打开数据库 |
| 规则集语义数据 | `rule-sets.json` | 保持规则集独立文件边界 |
| 编译后的规则集 | `.zrs` | 由 Zero 直接消费的版本化产物 |
| 导出的 Zero 运行配置 | 独立 JSON | 内核进程启动入口，不是 GUI 业务数据库 |
| 日志、调试帧、连接历史 | JSONL/独立文件 | 追加写与轮转模型，不属于配置关系数据 |

## 旧版本迁移

首次打开数据库时，Rust 会在单个事务中导入原有的 `proxy-configs.json` 和 `subscriptions.json`。导入成功后在数据库元数据中记录完成状态，后续启动不再重复导入；旧文件不会自动删除，可作为升级后的人工备份。缺失的代理配置引用会在迁移时清空。

`rule-sets.json` 不参与这次迁移。

## SQLite 加固

- 使用 `rusqlite` 的 bundled SQLite，避免依赖终端机器上版本不一致或缺失的动态库；
- 固定 application ID 和显式 schema version，拒绝误用其他应用数据库或打开更高版本 schema；
- 启用外键、`WAL`、`synchronous=FULL`、5 秒 busy timeout 和事务性迁移；
- 启用 defensive mode，关闭 trusted schema、writable schema 和双引号字符串兼容行为；
- 启用 secure delete、启动完整性快速检查、JSON 合法性/大小约束和订阅 URL 长度约束；
- 所有值都通过 SQL 参数绑定，不拼接订阅地址或配置内容；错误与日志不输出订阅地址；
- Unix 下数据库及 WAL/SHM 文件权限收紧为 `0600`；Windows 下继承应用数据目录的用户 ACL。

数据库不是用户配置接口。用户可编辑的简单设置仍通过 `app-config.json` 或 GUI 命令维护，数据库 schema 仅由 Rust 迁移管理。
