# 代理配置

代理配置属于 GUI Rust 应用层能力。前端不直接解析配置内容；Rust 负责解析能力、识别本地入站端口、持久化配置。

## 命令

| 命令 | 说明 |
| --- | --- |
| `proxy_config_list` | 列出所有代理配置 |
| `proxy_config_get` | 获取单个代理配置 |
| `proxy_config_upsert` | 创建或更新代理配置 |
| `proxy_config_import` | 从 JSON 文本或文件路径导入代理配置 |
| `proxy_config_set_active` | 设置 active 代理配置 |
| `proxy_config_remove` | 删除代理配置 |

## 调用参数

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `proxy_config_list` | 无 | `ProxyConfigProfile[]` |
| `proxy_config_get` | `{ id }` | `ProxyConfigProfile` |
| `proxy_config_upsert` | `{ input: ProxyConfigUpsert }` | `ProxyConfigProfile` |
| `proxy_config_import` | `{ input: ProxyConfigImport }` | `ProxyConfigProfile` |
| `proxy_config_set_active` | `{ id }` | `ProxyConfigProfile` |
| `proxy_config_remove` | `{ id }` | `void` |

## ProxyConfigProfile

```json
{
  "id": "proxy-config-1",
  "name": "默认配置",
  "kernel": "zero",
  "format": "zero-base64-json",
  "path": "https://example.com/sub",
  "content": {
    "inbounds": [],
    "outbounds": [],
    "route": {}
  },
  "active": true,
  "updatedAtUnixMs": 1713500000000,
  "capabilities": {
    "hasProxyNodes": true,
    "hasProxyGroups": true,
    "hasRouteRules": true,
    "hasRuleSets": false,
    "hasSelector": true,
    "hasUrlTest": false,
    "featureKeys": ["proxyNodes", "proxyGroups"]
  }
}
```

## Upsert

```ts
await invoke('proxy_config_upsert', {
  input: {
    id: 'proxy-config-1',
    name: '默认配置',
    kernel: 'zero',
    format: 'json',
    path: null,
    content: { inbounds: [], outbounds: [], route: {} },
    active: true
  }
});
```

## Import

`proxy_config_import` 支持：

- `content`: JSON 字符串
- `path`: 本地 JSON 文件路径

二者至少提供一个。当前只支持 JSON，不支持 YAML。

```ts
await invoke('proxy_config_import', {
  input: {
    name: '本地配置',
    kernel: 'zero',
    format: 'json',
    content: '{"inbounds":[],"outbounds":[],"route":{}}',
    active: true
  }
});
```

## 本地入站端口同步

Rust 会从 zero JSON 中读取：

```json
{
  "inbounds": [
    {
      "listen": {
        "address": "127.0.0.1",
        "port": 7891
      }
    }
  ]
}
```

当配置被设为 active 时，Rust 会同步：

```json
{
  "localProxy": {
    "host": "127.0.0.1",
    "port": 7891,
    "sourceProxyConfigId": "proxy-config-1"
  }
}
```

如果配置没有 `inbounds[].listen.port`，则不覆盖应用层本地代理配置。

## 能力识别规则

Rust 当前按以下 JSON 结构识别能力：

| 能力 | 识别字段 |
| --- | --- |
| `proxyNodes` | `proxies` 或 `outbounds` 非空 |
| `proxyGroups` | `proxy-groups`、`proxy_groups`、`policy_groups` 或 `policies` 非空 |
| `routing` | `rules` 或 `route.rules` 非空 |
| `ruleSets` | `rule-providers`、`rule_providers`、`rule_sets` 或 `ruleSets` 非空 |
| `selector` | 配置中存在 `select` 或 `selector` 类型 |
| `urlTest` | 配置中存在 `url-test`、`urltest` 或 `url_test` 类型 |
