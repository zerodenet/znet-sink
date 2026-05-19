# 日志

日志属于 GUI Rust 应用层能力。当前日志为内存环形列表，最大条数由 `AppConfig.logs.maxEntries` 控制。

## 命令

| 命令 | 说明 |
| --- | --- |
| `logs_list` | 查询日志 |
| `logs_append` | 追加日志 |
| `logs_clear` | 清空日志 |

## 调用参数

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `logs_list` | `{ query?: LogQuery }` | `LogEntry[]` |
| `logs_append` | `{ input: LogAppend }` | `LogEntry` |
| `logs_clear` | 无 | `void` |

## LogEntry

```json
{
  "id": 1,
  "source": "core",
  "level": "info",
  "message": "core process started",
  "fields": {
    "pid": 1234
  },
  "occurredAtUnixMs": 1713500000000
}
```

## source

| 值 | 说明 |
| --- | --- |
| `app` | GUI 应用日志 |
| `core` | 内核相关日志 |

## level

可选：

- `trace`
- `debug`
- `info`
- `warn`
- `error`

## 查询

```ts
await invoke('logs_list', {
  query: {
    source: 'core',
    level: 'error',
    limit: 100
  }
});
```

`limit` 默认 200，最大 1000。
