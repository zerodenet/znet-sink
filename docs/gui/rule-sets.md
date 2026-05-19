# 规则集配置

规则集配置属于 GUI Rust 应用层能力，当前是配置管理和持久化脚手架，不直接写入内核运行状态。

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
  "id": "rule-set-1",
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
