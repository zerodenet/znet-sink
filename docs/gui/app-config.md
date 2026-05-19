# 应用配置

应用配置属于 GUI Rust 应用层能力，不依赖内核 IPC。配置会持久化为 GUI 自己的 `app-config.json`。

## 命令

| 命令 | 说明 |
| --- | --- |
| `app_config_get` | 获取当前应用配置 |
| `app_config_update` | 合并更新应用配置并持久化 |

## 调用参数

| 命令 | 入参 | 返回 |
| --- | --- | --- |
| `app_config_get` | 无 | `AppConfig` |
| `app_config_update` | `{ patch: AppConfigPatch }` | 更新后的 `AppConfig` |

调用示例：

```ts
const config = await invoke('app_config_get');

await invoke('app_config_update', {
  patch: {
    ui: {
      sidebarCollapsed: true,
      hiddenMenuKeys: ['logs', 'ruleSets']
    }
  }
});
```

## AppConfig

```json
{
  "schemaVersion": "gui.app.v1",
  "core": {
    "kernel": "zero",
    "autoConnect": true,
    "autoStart": false,
    "executablePath": null,
    "configPath": null,
    "workingDir": null,
    "socket": null
  },
  "logs": {
    "level": "info",
    "maxEntries": 500
  },
  "ui": {
    "sidebarCollapsed": false,
    "hiddenMenuKeys": [],
    "defaultRoute": null
  },
  "localProxy": {
    "host": "127.0.0.1",
    "port": 7890,
    "sourceProxyConfigId": null
  }
}
```

## 字段说明

### `core`

| 字段 | 说明 |
| --- | --- |
| `kernel` | 当前内核类型，现阶段固定为 `zero` |
| `autoConnect` | 前端是否可默认尝试连接运行中内核 |
| `autoStart` | 应用启动时是否由 GUI 后台托管启动内核 |
| `executablePath` | zero 可执行文件路径；为空时默认指向项目根目录 `build/core/zero.exe`，非 Windows 为 `build/core/zero` |
| `configPath` | zero 启动配置文件路径，由 `core_config_export_active` 更新 |
| `workingDir` | 内核工作目录；为空时使用可执行文件所在目录 |
| `socket` | IPC socket/named pipe 覆盖路径 |

`socket` 为空时的默认策略：

- Windows 使用内核默认 named pipe：`\\.\pipe\zero-control`。
- 非 Windows 使用 zero 可执行文件同目录下的 `zero-control.sock`，并在启动参数中显式传入 `--control-socket`。

### `ui`

UI 偏好是用户偏好，不等价于业务能力。

| 字段 | 说明 |
| --- | --- |
| `sidebarCollapsed` | 侧栏是否折叠 |
| `hiddenMenuKeys` | 用户手动隐藏的菜单 key，Rust 会去空、去重、排序 |
| `defaultRoute` | 用户默认打开路由 |

能力驱动的菜单显隐应使用 [能力快照](./capabilities.md)，不要写入 `hiddenMenuKeys`。

### `localProxy`

本地客户端代理入口。默认是 `127.0.0.1:7890`。

当 active 代理配置中存在 `inbounds[].listen.port` 时，Rust 会解析并同步到 `localProxy`。例如配置指定 `7891`，应用配置也会更新为 `7891`。

| 字段 | 说明 |
| --- | --- |
| `host` | 本地代理监听地址 |
| `port` | 本地代理监听端口，范围 1-65535 |
| `sourceProxyConfigId` | 当前端口来源的代理配置 ID |

## Patch 规则

`app_config_update` 是浅层分组 patch。只传需要更新的分组和字段即可。

可空字符串字段的约定：

- 不传字段：保持原值。
- 传空字符串：清空该字段。
- 传字符串：更新该字段。

例如清空默认路由：

```ts
await invoke('app_config_update', {
  patch: {
    ui: {
      defaultRoute: ''
    }
  }
});
```

当前 Rust patch 结构没有启用 double-option 反序列化，`null` 会按“不传字段”处理。
