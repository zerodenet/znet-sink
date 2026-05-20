# 代理配置

代理配置属于 GUI Rust 应用层能力。前端不直接解析配置内容；Rust 负责解析能力、识别本地入站端口、持久化配置。

`ProxyConfigProfile.id` 是 GUI 存储主键，只用于前端调用 `get` / `set_active` / `remove` 等命令；它不是 zero 内核配置里的 `tag` 或 `name`，也不参与路由引用。zero 内核语义仍以 `content` 内部的 `tag` / `name` / `route` 为准。

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
  "id": "proxy-config_18f6b2a7c9f42",
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
    id: 'proxy-config_18f6b2a7c9f42',
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

## 代理模式切换

前端不应直接改写 `content.route`。代理模式切换使用 Zero 适配层命令：

| 命令 | 说明 |
| --- | --- |
| `gui_proxy_mode_status` | 读取 active 配置的当前代理模式 |
| `gui_set_proxy_mode` | 写入代理模式、保存 active 配置、导出给 Zero |

支持的 GUI 模式：

| 模式 | 语义 | 当前 zero 0.0.3 写入方式 |
| --- | --- | --- |
| `global` | 全局代理 | `mode = { "type": "global", "outbound": "<globalOutbound>" }` |
| `rule` | 规则分流 | `mode = { "type": "rule" }`，并保留 `route.rules` 和既有 `route.final` |
| `direct` | 全部直连 | `mode = { "type": "direct" }` |

切换模式不删除规则、规则集、策略组或节点。简约模式和专业模式都可以使用该能力；专业模式仍可管理规则等高级配置。

当前打包的 zero 0.0.3 实测支持顶层 `mode` 字段；Rust 读取状态时也兼容旧配置中的 `route.final`。
当前 zero 0.0.3 仍要求配置中存在 `route`；Rust 写入顶层 `mode` 时会保留或补齐 `route.final`。

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
