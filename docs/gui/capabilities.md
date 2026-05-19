# 能力快照

能力快照由 Rust 侧根据当前应用状态和 active 代理配置生成。前端根据快照决定能力卡片、菜单项和操作入口是否展示或禁用。

## 命令

| 命令 | 说明 |
| --- | --- |
| `gui_capabilities_snapshot` | 获取当前 GUI 能力快照 |

## 调用参数

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `gui_capabilities_snapshot` | 无 | `GuiCapabilitySnapshot` |

## GuiCapabilitySnapshot

```json
{
  "management": [
    { "key": "proxyConfig", "enabled": true, "reason": null },
    { "key": "subscriptions", "enabled": true, "reason": null },
    { "key": "appLogs", "enabled": true, "reason": null },
    { "key": "coreLogs", "enabled": true, "reason": null },
    { "key": "appConfig", "enabled": true, "reason": null },
    { "key": "ruleSets", "enabled": true, "reason": null }
  ],
  "proxyFeatures": [
    { "key": "routing", "enabled": true, "reason": null },
    { "key": "ruleSets", "enabled": false, "reason": "active proxy config does not define ruleSets" },
    { "key": "selector", "enabled": true, "reason": null },
    { "key": "urlTest", "enabled": false, "reason": "active proxy config does not define urlTest" }
  ],
  "activeProxyConfigId": "proxy-config-1",
  "activeProxyConfigCapabilities": {
    "hasProxyNodes": true,
    "hasProxyGroups": true,
    "hasRouteRules": true,
    "hasRuleSets": false,
    "hasSelector": true,
    "hasUrlTest": false,
    "featureKeys": ["proxyNodes", "proxyGroups", "routing", "selector"]
  }
}
```

## 前端使用原则

- `management` 表示 GUI 管理域是否可用。
- `proxyFeatures` 表示 active 代理配置实际提供的代理能力。
- `enabled=false` 时，前端可以隐藏入口，也可以展示禁用态。
- 前端不要直接解析代理配置判断 `urlTest`、`selector`、`routing` 等能力。

## 当前能力 key

| key | 说明 |
| --- | --- |
| `proxyConfig` | 代理配置管理 |
| `subscriptions` | 订阅管理 |
| `appLogs` | 应用日志 |
| `coreLogs` | 内核日志 |
| `appConfig` | 应用配置 |
| `ruleSets` | 规则集管理 |
| `routing` | 路由/分流能力 |
| `selector` | 手动选择出口能力 |
| `urlTest` | 延迟测试/自动选优能力 |
