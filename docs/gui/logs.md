# 日志

日志属于 GUI Rust 应用层能力。日志会同时保存在内存环形列表和本地 `logs.jsonl` 文件中，最大保留条数由 `AppConfig.logs.maxEntries` 控制。

默认存储目录：

```text
%APPDATA%\ZNet Sink\logs.jsonl
```

如果设置了 `ZNET_SINK_DATA_DIR`，则写入该目录下的 `logs.jsonl`。

## 命令

| 命令 | 说明 |
| --- | --- |
| `logs_list` | 查询日志 |
| `logs_append` | 追加日志 |
| `logs_clear` | 清空日志 |

应用启动时会从 `logs.jsonl` 加载最近 `maxEntries` 条日志，并压缩文件到最近 `maxEntries` 条。`logs_append`、内核 stderr 捕获、一键连接/断开等后端日志都会追加写入该文件；每次追加后都会执行轮转，避免文件无限增长。`logs_clear` 会同时清空内存日志和文件内容。

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
