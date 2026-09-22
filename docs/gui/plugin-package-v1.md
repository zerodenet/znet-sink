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

组件 manifest 的 `source_sha256` 是入口文件**原始字节**的 SHA-256；每次修改入口后必须重算。根清单的文件索引另行覆盖导入模块和 UI 文件。组件 manifest 还需声明 `requires_host`、`api_version: 1`、`minimum_isolation: "vm"`、`targets`、`required`、`optional` 与 `limits`；精确示例见仓库样本。早期 `javascript-v1` 单脚本只用于旧插件过渡，不是新插件模板。

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
