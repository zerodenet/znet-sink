# 订阅管理

订阅管理属于 GUI Rust 应用层能力。当前 GUI 只支持自研 zero 内核订阅格式。

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

当前严格支持 `base64(JSON)`。

`format` 可选值：

| 值 | 说明 |
| --- | --- |
| `auto` | 按 zero base64 JSON 解析 |
| `zero` | 按 zero base64 JSON 解析 |
| `zero-base64-json` | 明确指定 zero base64 JSON |
| `base64-json` | base64 JSON 别名 |

不支持：

- 明文 JSON
- Clash YAML
- 通用节点 URI 列表
- base64 节点 URI 列表

订阅内容解码后必须是 JSON object。

## 同步流程

`subscription_sync` 执行：

1. HTTP GET 拉取 `url`。
2. 去除响应空白。
3. 按 base64 解码，支持 standard 和 URL-safe。
4. UTF-8 解码。
5. JSON object 解析。
6. 创建或更新目标 `ProxyConfigProfile`。
7. 解析能力和本地入站端口。
8. 持久化代理配置和订阅同步状态。

同步成功更新：

- `targetProxyConfigId`
- `lastSyncAtUnixMs`
- `lastError = null`

同步失败更新：

- `lastError`

同步生成的代理配置默认不会自动设为 active；如果已有 `targetProxyConfigId` 且该代理配置原本是 active，则同步后会保持 active 并同步 `localProxy`。
