# ZNet Sink 插件 SDK v1

状态：客户端通用宿主接口已实现，Rust 与 TypeScript 契约位于 `sdk/rust` 和 `sdk/typescript`。本文只定义宿主能力，不包含 Connect、ZBoard、账号、订阅或任何特定业务协议。

## 身份与调用模型

插件包声明组件、surface、权限和精确 scope。客户端从已验签的组件或管理页面上下文确定 `plugin_id/component_id`，SDK 请求不能提交或替换这两个身份，也不能拼接 Tauri 命令。每个请求固定包含：

- `version`：当前为 `1`；未知版本直接拒绝。
- `request`：一个已声明、已授权的 `capability + scope`。
- `method`：SDK v1 的固定方法枚举。
- `arguments`：最多 256 KiB 的结构化参数。
- `budget`：1–120 秒的时间预算及最多 1 MiB 的结果预算；客户端在返回结果前再次检查预算和授权代次。

停止、撤权、升级、卸载或客户端退出会撤销组件租约。宿主不会把租约、主密钥或平台凭证交给插件；撤销后产生的晚到结果不得提交。用户取消选文件会返回 `cancelled`，时间与大小限制分别返回 `deadline` 和 `budget_exceeded`。外部副作用是否已经发生仍由对应服务的幂等键或状态查询判断，SDK 不声称提供跨服务 exactly-once。

## 能力

| 能力 | scope | 宿主行为 |
| --- | --- | --- |
| `plugin.storage.read/write` | `self` | 发布者指纹和插件 ID 隔离的 state/cache；列举、导出、清空和逐版本迁移；值为插件定义的 Base64 不透明内容 |
| `notifications.post` | `self` | 显示带插件来源的客户端通知；每组件每分钟 3 条、每小时 20 条 |
| `tasks.schedule` | `self` | 保存 5 分钟至 7 天的有限调度意图；执行时重新检查启用状态和权限；停用、撤权、卸载清理 |
| `browser.open` | 精确 HTTP(S) origin | 客户端验证目标仍属于获批 origin 后使用平台打开服务 |
| `browser.callback` | `self` | 创建 127.0.0.1 随机端口、随机路径的一次性回调；5 分钟到期；查询结果只回到所属组件 |
| `files.selection.read/write` | `user` | 客户端先展示原生选择器，再生成不可由插件指定的读取/发布对象；单次最多 4 MiB，写入采用原子发布 |
| `materials.submit` | `configuration` | 把页面内已有材料转换为最多 10 分钟的内存句柄；句柄按组件隔离并可显式销毁 |
| `secrets.session.receive` | 精确 HTTP(S) origin | 客户端直接接收远端响应并存入短期 `secret` 句柄，不把响应正文返回 JS |
| `crypto.session.use` | `self` | 对短期句柄执行 HMAC-SHA256 或 ChaCha20-Poly1305 校验解密；解密结果仍为短期句柄 |
| `runtime.protected.load` | `active-runtime` | 一次性消费 `material` 句柄，完成配置合成、校验与 runtime-only 应用；只返回安全运行态投影 |

普通插件存储不等同于安全保险箱。长期敏感内容必须由插件或远端服务生成密文/不透明信封后再保存。客户端负责命名空间、容量、原子性和生命周期，不定义业务密钥、Connect 加密协议或 ZBoard 登录流程。

## 管理页面

管理页面来自已验签安装包，运行在不带同源权限的 iframe 中。CSP 禁止页面直接联网、提交表单、加载远端脚本和调用 Tauri。客户端注入 `znetPlugin`，其配置、存储、通知、调度、浏览器、文件、材料和组件调用均绑定当前插件；插件不能指定另一插件身份。

页面可以自行渲染登录、资源选择、自检和业务说明。名称、版本、发布者、启用状态、权限、更新和卸载仍由客户端固定详情页管理。

## 权限生命周期

1. 安装或升级先比较包内声明与线上登记上限；本地包使用显式本地发布者信任。
2. 客户端展示必需/可选权限和精确 scope，用户批准后持久保存已审声明和授权集合。
3. 每次 SDK 调用重新检查安装身份、组件状态、声明、授权和当前租约。
4. 升级新增能力或扩大 scope 必须重新确认；新包不会继承未审权限。
5. 删除单项权限会清理依赖该权限的回调、短期材料或调度；停用、撤权、卸载清理该组件的活动资源。

## SDK 位置

- Rust：`sdk/rust`，提供独立 wire contract、`Transport` 和类型化 `Client`，不依赖 Tauri 或插件沙箱实现。
- TypeScript：`sdk/typescript/index.ts`，提供 `createSdk(transport)` 及类型化 capability 方法。
- 管理页运行时：客户端注入等价的 `globalThis.znetPlugin`，页面无需接触宿主命令名。

新增方法或能力必须提升契约版本或保持向后兼容，并同步更新 Rust、TypeScript、宿主映射、权限文案、负面权限测试和生命周期清理测试。
