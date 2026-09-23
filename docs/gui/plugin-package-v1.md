# ZNet Sink 插件应用包 v1：作者交接规范

这是新插件的唯一打包规范。可运行的最小示例位于 [`examples/plugin-package-v1/`](../../examples/plugin-package-v1/)；客户端仍可读取早期 JSON 开发包以便 Connect 过渡，但新插件不要再生成 JSON 信封包。

## 文件与身份

```text
plugin.json
components/<component_id>/manifest.json
components/<component_id>/index.js
components/<component_id>/lib/*.js
ui/<page_id>/index.html
ui/<page_id>/*.css
ui/<page_id>/*.js
META-INF/signature.json     # 打包工具生成，不放进源码目录
```

`plugin.json` 使用 `schema_version: 1`、`host: "znet-sink"`、稳定的 `plugin_id`、插件 `version`、`components`、可选 `pages` 和空的 `files: {}`。每个组件描述符指定 `id`、`manifest`、`entry`；组件 manifest 的 `plugin_id`、`component_id`、`version` 必须与根清单一致。每个页面描述符指定 `id`、`title`、`kind: "management"`、HTML `entry`，可选 `styles` 与 `scripts` 路径数组。路径均相对包根，不能使用绝对路径、`..` 或反斜杠。

`pack` 自动生成覆盖每个内容文件的 SHA-256/字节数索引，并以 Ed25519 签署根清单。包为单个 `.zspkg` ZIP，签名信封的 `format` 是 `znet-sink.plugin-package.v1`。客户端校验发布者身份、签名、全部文件、能力上限和设备/宿主兼容性后才安装；只改文件或只改清单都会验签失败。

## 组件运行时

新组件使用 `runtime: "javascript-module-v1"`。`entry` 必须位于 `components/<component_id>/`，导出同步的默认函数；它可以从该组件目录内以相对路径静态导入 `.js`/`.mjs`。例如：

```js
import { message } from './lib/message.js';
export default function () {
  return { action: pluginInput.invocation?.action, message };
}
```

宿主在调用前设置只读全局 `pluginInput`。页面调用 `znetPlugin.invoke(componentId, action, payload)` 时，其值包含 `configuration`、`state`、`invocation: { action, payload }`；后台计划任务同样使用宿主提供的调用输入。返回值必须可序列化为 JSON。若要原子提交插件命名空间状态，可返回 `{ "znet_plugin_result": 1, "state_updates": { "key": "value" }, "value": ... }`；否则返回值会直接交给调用方。不要在模块内依赖文件系统、`fetch`、Node API、动态导入、待决 Promise 或长期驻留进程；它们未开放。宿主能力只能经已授权的宿主桥/SDK 调用。

## 插件运行日志

组件 manifest 需在 `required` 或 `optional` 声明 `{ "capability": "plugin.logs.write", "scope": "self" }`，并由用户授权。管理页面可调用 `await znetPlugin.logs.write(componentId, 'info', '同步完成', { count: 3 })`。组件 VM 的普通调用和计划任务都可同步调用 `hostSdkCall(JSON.stringify({ version: 1, request: { capability: 'plugin.logs.write', scope: 'self' }, method: 'log_write', arguments: { level: 'info', message: '同步完成', fields: { count: 3 } } }))`，返回 SDK `Reply` JSON 字符串；应检查 `ok`，失败时处理 `error.code`。Rust SDK 提供 `Client::log(level, message, fields)`。

允许级别为 `trace`、`debug`、`info`、`warn`、`error`。消息最多 2 KiB，结构化字段 JSON 最多 4 KiB，每组件每分钟最多 120 条。宿主会附加可信的 `pluginId`、`componentId` 并使用独立的 `plugin` 日志来源，插件不能指定来源或冒充其他插件；自定义字段嵌套在 `data`。日志使用客户端现有的脱敏、留存和清理策略。不要写入令牌、订阅内容、私钥或响应正文，脱敏是最后一道防线。宿主还会记录页面调用、手动运行和后台计划任务的结果以及 SDK 失败（不记录输入负载）；运行日志页可筛选“插件”，并显示所属组件。旧包若未声明此权限，必须重新打包并经用户重新授权后才能主动写日志。

组件 manifest 的 `source_sha256` 是入口文件**原始字节**的 SHA-256；每次修改入口后必须重算。根清单的文件索引另行覆盖导入模块和 UI 文件。组件 manifest 还需声明 `requires_host`、`api_version: 1`、`minimum_isolation: "vm"`、`targets`、`required`、`optional` 与 `limits`；精确示例见仓库样本。早期 `javascript-v1` 单脚本只用于旧插件过渡，不是新插件模板。

组件若依赖某个具体 SDK 方法，还必须在 `requires_methods` 中列出 snake_case 方法名，例如 `"subscription_metadata_update"`。宿主在安装时反序列化并校验这些方法，且要求它们对应的 capability 同时出现在 `required` 或 `optional` 权限声明中。旧客户端因 manifest 拒绝未知字段而不会安装带此声明的新包，当前客户端也会拒绝未知方法或缺少匹配权限的包；因此方法缺失会在安装期暴露，而不是运行到同步流程后才失败。`requires_host` 仍用于声明最低产品版本，两者应同时维护。

## 托管订阅只读元数据

拥有 `{ "capability": "subscriptions.manage", "scope": "self" }` 且在 manifest 声明 `requires_methods: ["subscription_metadata_update"]` 的组件，可以独立更新自己命名空间下已有托管订阅的服务元数据。管理页面调用 `znetPlugin.subscriptions.updateMetadata(componentId, providerId, remoteSubscriptionId, usage)`；组件 VM 使用同名 SDK method，Rust SDK 使用 `Client::update_subscription_metadata`，TypeScript SDK 使用 `subscriptions.updateMetadata`。`usage` 仅包含 `usedBytes`、`totalBytes`、`expireAtUnixMs`；`usedBytes` 是服务端报告的累计总用量，不会被拆成上传/下载。

宿主从已验证会话推导插件身份，并以 `plugin_id + provider_id + remote_subscription_id` 查找目标。此方法不会接收订阅内容、重建配置、改变活动配置或覆盖策略选择。失败对象只公开 `code`、`message`、可选 `field_path`、`diagnostics` 和 `retry_after_ms`。

## 管理页面

页面的 HTML、CSS 和经典 JS 分别存文件，`styles` / `scripts` 在 `plugin.json` 中显式列出。客户端验签后把这些资源装入隔离 iframe；页面只能通过注入的 `znetPlugin` API 与宿主交互。`znetPlugin.invoke` 的 `action` 限 128 字节且只接受 ASCII 字母、数字、点、下划线和连字符，`payload` 的 JSON 不超过 16 KiB。页面资源不能直接访问本机文件或网络；页面端 ES Module、常驻 worker/service 和本地监听尚未开放。线上发布管理页面时，插件中心登记还需声明 `znet-sink.ui.management.v1` surface。

## 离线打包与交接

在 GUI 仓库构建 `znet-plugin` 后执行：

```sh
znet-plugin pack APP_ROOT PRIVATE_SEED_FILE PACKAGE_OUT METADATA_OUT [REGISTRATION_JSON]
```

`APP_ROOT` 是含 `plugin.json` 的目录；32 字节原始私钥 seed、发布者登记 JSON、输出包和元数据都必须放在该目录之外。输出路径必须尚不存在。带 `REGISTRATION_JSON` 的包可走用户确认发布者的本地安装流程；没有它时，客户端需要插件中心已有登记来取得验签公钥和权限上限。命令只在本地打包，不会上传、发布、授权或运行插件。

交接给客户端验收时提供 `.zspkg`、元数据 JSON、发布者登记/公钥、预期 `plugin_id` 和所需权限。客户端对真实插件的安装、授权、页面 RPC、后台任务和业务操作仍需单独端到端验收；打包成功不等于运行验收通过。

包上限 16 MiB、256 个文件、单文件 4 MiB、总解压 32 MiB；模块源码最多 64 个文件/4 MiB，入口源码最多 256 KiB；页面 CSS/JS 最多各 8 个，单项 512 KiB、单页合计 2 MiB。旧 JSON 包仅作读取和 `pack-legacy` 过渡，不能声明模块运行时。
