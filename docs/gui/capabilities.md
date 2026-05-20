# 能力与交互面

能力快照由 Rust 侧根据当前应用状态和 active 代理配置生成。它是内部诊断接口，不应作为用户菜单或常规页面入口。

菜单、页面入口和操作准入应使用 `gui_interaction_surface_snapshot`。简约模式与专业模式的语义见 [交互模式约束](./interaction-modes.md)。前端可以根据交互面渲染不同布局，但不要在组件中复制模式规则。

## 命令

| 命令 | 说明 |
| --- | --- |
| `gui_capabilities_snapshot` | 获取当前 GUI 能力快照，仅用于内部诊断 |
| `gui_interaction_surface_snapshot` | 获取已合并 `uiMode` 与 Zero 能力声明的交互面 |

## 调用参数

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `gui_capabilities_snapshot` | 无 | `GuiCapabilitySnapshot` |
| `gui_interaction_surface_snapshot` | 无 | `InteractionSurfaceSnapshot` |

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

- 常规菜单不要展示“能力”页面。
- `gui_capabilities_snapshot` 仅用于内部诊断、开发排查或调试日志，不面向普通用户和专业用户。
- `management` 表示 GUI 管理域是否可用。
- `proxyFeatures` 表示 active 代理配置实际提供的代理能力。
- `enabled=false` 时，诊断视图可以展示禁用态。
- 前端不要直接解析代理配置判断 `urlTest`、`selector`、`routing` 等能力。
- 前端不要自行定义“简约模式隐藏哪些高级能力”；该结论应来自 Rust 返回的交互面快照。

## InteractionSurfaceSnapshot

```json
{
  "uiMode": "lite",
  "navigation": [
    { "key": "overview", "category": "navigation", "visible": true, "operable": true, "readonly": false, "reason": null },
    { "key": "rules", "category": "navigation", "visible": false, "operable": false, "readonly": false, "reason": "hidden in lite mode" }
  ],
  "actions": [
    { "key": "core.overview", "category": "action", "visible": true, "operable": true, "readonly": false, "reason": null },
    { "key": "selfTest.snapshot", "category": "action", "visible": true, "operable": true, "readonly": false, "reason": null },
    { "key": "proxyMode.set", "category": "action", "visible": true, "operable": true, "readonly": false, "reason": null }
  ],
  "features": [
    { "key": "selfTest", "category": "feature", "visible": true, "operable": true, "readonly": false, "reason": null },
    { "key": "proxyMode", "category": "feature", "visible": true, "operable": true, "readonly": false, "reason": null },
    { "key": "connections", "category": "feature", "visible": true, "operable": false, "readonly": true, "reason": "zero capability does not declare any of: flow-snapshot" }
  ]
}
```

`visible=false` 表示当前模式下不展示入口；`operable=false` 表示入口不可执行写入或控制操作；`readonly=true` 表示可以展示状态但不能操作。

## 诊断能力 key

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
