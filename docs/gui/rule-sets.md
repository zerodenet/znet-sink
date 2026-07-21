# 规则集配置

规则集页面只管理由 GUI 创建或导入的公共规则资源。节点订阅随附的规则提供者属于该订阅的内部资源：同步时由应用层下载、转换并注入对应订阅配置，但不会通过 `rule_set_list` / `rule_set_get` 暴露，也不会出现在规则集页面。

`RuleSetProfile.id` 是 GUI 存储主键，只用于管理这条规则集记录；它不是 zero 内核规则集 `tag`。后续如果要把 GUI 规则集注入 zero 配置，需要单独定义 `tag` 映射规则。

## 命令

| 命令 | 说明 |
| --- | --- |
| `rule_set_list` | 列出规则集 |
| `rule_set_get` | 获取单个规则集 |
| `rule_set_upsert` | 创建或更新规则集 |
| `rule_set_remove` | 删除规则集 |

## 调用参数

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `rule_set_list` | 无 | `RuleSetProfile[]` |
| `rule_set_get` | `{ id }` | `RuleSetProfile` |
| `rule_set_upsert` | `{ input: RuleSetUpsert }` | `RuleSetProfile` |
| `rule_set_remove` | `{ id }` | `void` |

## RuleSetProfile

```json
{
  "id": "rule-set_18f6b2a7c9f42",
  "name": "GeoIP CN",
  "format": "json",
  "enabled": true,
  "source": {
    "kind": "remote",
    "url": "https://example.com/rules.json",
    "path": null,
    "content": null
  },
  "updatedAtUnixMs": 1713500000000
}
```

## Source

| kind | 必填字段 | 说明 |
| --- | --- | --- |
| `remote` | `url` | 远程规则集，URL 必须是 `http://` 或 `https://` |
| `file` | `path` | 本地文件 |
| `inline` | `content` | 内联 JSON 内容 |

当前规则集不会自动注入 active 代理配置。前端如需“编辑规则集并生成 zero 配置”，需要另行定义转换规则。
