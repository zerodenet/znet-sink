# 订阅管理

订阅管理属于 GUI Rust 应用层能力。GUI 接收 Zero 订阅，并将支持范围内的 Clash 订阅转换为 Zero 配置。

`SubscriptionProfile.id` 和 `targetProxyConfigId` 都是 GUI 存储主键，用于管理订阅记录和其生成的代理配置记录；它们不是 zero 内核 `tag`。

## 命令

| 命令 | 说明 |
| --- | --- |
| `subscription_list` | 列出订阅 |
| `subscription_get` | 获取单个订阅 |
| `subscription_upsert` | 创建或更新订阅 |
| `subscription_sync` | 拉取订阅并生成/更新代理配置 |
| `subscription_remove` | 删除订阅 |

## 调用参数

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `subscription_list` | 无 | `SubscriptionProfile[]` |
| `subscription_get` | `{ id }` | `SubscriptionProfile` |
| `subscription_upsert` | `{ input: SubscriptionUpsert }` | `SubscriptionProfile` |
| `subscription_sync` | `{ id }` | `SubscriptionProfile` |
| `subscription_remove` | `{ id }` | `void` |

## SubscriptionProfile

```json
{
  "id": "subscription_18f6b2a7c9f42",
  "name": "机场订阅",
  "url": "https://example.com/sub",
  "enabled": true,
  "kernel": "zero",
  "format": "auto",
  "targetProxyConfigId": "proxy-config_18f6b2a7c9f43",
  "updatedAtUnixMs": 1713500000000,
  "lastSyncAtUnixMs": 1713500000000,
  "lastError": null
}
```

## 支持的订阅格式

| 值 | 说明 |
| --- | --- |
| `auto` | 自动识别 Zero base64 JSON、Clash YAML 或 base64 YAML |
| `zero` | Zero base64 JSON；不接收明文 Zero JSON |
| `clash` | Clash 明文或 base64 YAML，转换为 Zero 配置 |

存储层兼容 `zero-base64-json`、`base64-json`、`clash-yaml`、`clash-base64-yaml` 等历史别名。通用节点 URI 列表不在支持范围。

Clash 路由转换支持三段式 `DOMAIN`、`DOMAIN-SUFFIX`、`DOMAIN-KEYWORD`、`IP-CIDR`、`IP-CIDR6`、`GEOIP`、`RULE-SET`，以及末尾的两段式 `MATCH`。规则目标必须存在。额外修饰参数（包括 `no-resolve`）、未支持的规则类型、错误目标、非末尾 MATCH 均明确拒绝；不会删除这些规则后宣称成功。此处声明的是转换范围，实际 GEOIP 数据、协议与运行配置仍需通过 Zero 校验。

`RULE-SET` 需要可用的 provider 定义及对应本地 ZRS。provider 更新失败时可以继续使用已有的已验证产物；没有可用产物则同步失败。该限制同样适用于 Zero 配置中的嵌套规则集引用。失败信息列出缺失规则集，原有拒绝规则保持不变。

## 同步流程

1. 拉取并按所选格式解码、解析订阅；自定义 User-Agent 完全覆盖默认值。
2. 转换配置并检查路由完整性，同步托管规则集依赖。
3. 构建候选配置，保留客户端管理的本地入口和全局 DNS 设置。
4. 若目标是运行中的活动配置，先校验并应用，再核对内核实例和配置版本。失败或超时不会自动重启内核。
5. 保存代理配置，更新托管系统代理，再记录订阅同步成功；失败仍返回错误。

这些步骤不代表所有存储文件和操作系统操作构成原子事务；订阅规则集产物的更新与配置提交之间仍需覆盖进程中断验收。

同步成功更新：

- `targetProxyConfigId`
- `lastSyncAtUnixMs`
- `lastError = null`

同步失败更新：

- `lastError`

同步生成的代理配置默认不会自动设为 active；如果已有 `targetProxyConfigId` 且该代理配置原本是 active，则同步后会保持 active 并同步 `localProxy`。
