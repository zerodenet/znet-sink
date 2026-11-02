# 全局用户端点

专业模式端点页的创建、编辑与删除操作由客户端管理全局定义，并在配置合成时
加入所有当前/后续配置。不修改订阅原文，不按配置逐份绑定。

## 管理契约

- `gui_user_endpoints` 返回 `{ revision, endpoints, protocols }`。
- `gui_user_endpoints_save` 接收 `{ expectedRevision, endpoints }`，返回同一快照形状。
- `revision` 用于阻止陈旧草稿覆盖其他窗口及已确认的启停/方向修改。
- `endpoints` 使用通用声明信封：`tag`、可选 `enabled`、可选 `directions` 与
  `protocol: { type, ...协议材料 }`。材料可能包含密钥，不进入通知或日志。
- `protocols` 为 Rust 已实现的客户端编辑适配器目录：`[{ type, label }]`。
  这不是内核运行能力声明；实际运行能力仍来自内核契约及配置预检。
- 当前只登记 WireGuard。来源配置和运行态目录中的其他协议仍可观测，不据此
  自动开放创建，更不从协议名猜测可执行操作。

保存持有配置操作锁，先核对端点库版本、名称冲突与移除依赖，再合成有效配置。
运行时先校验并复用现有应用事务；离线保存供下次启动使用。不确定结果不重放。
详细合并和应用边界见[验证说明](../verification/global-user-endpoints.md)。

## 前端与扩展边界

- 工具栏提供“创建端点”和“导入配置”；用户创建的端点在原有卡片菜单中提供编辑、删除。
  未确认运行态的定义仍可离线管理，但不显示虚构的计数或运行状态。
- 通用弹窗负责名称、协议选择、启用、方向、草稿、提交与删除；不读取协议私钥、
  地址或 peer。编辑已存在的端点时协议固定，切换协议须新建定义。
- 协议选项必须同时出现在 Rust 目录与前端表单适配表中。缺少编辑器时显示升级
  提示，保留材料，不把未知协议解释成 WireGuard。
- WireGuard 的类型、默认草稿、字段表单及 Rust 校验分别位于协议适配文件中。
  新协议接入需增加后端适配/元数据与前端表单，核对真实内核字段；复用全局库、
  版本检查、合并、应用及卡片生命周期。不要仅增加一个协议名称来宣告支持。
- 复用现有 `Input`、`Switch`、`FieldSelect`、`Dialog` 和 `Button`，使用主题 token。
  长表单在正文区域滚动，底部操作固定；提交中阻止关闭，失败保留草稿。

## WireGuard 配置导入与字段

- 端点页的“导入配置”支持同时选择多个 `.conf`/`.ini` 文件。先预览地址、Peer 数量
  和端点名称，再确认保存；名称冲突须修改后再提交，不覆盖已有端点。一个文件失败
  时整批暂不保存；版本冲突或应用失败保留预览，不自动重试。
- 创建和编辑窗口也可“从 .conf 文件填写”，替换连接参数，保留已输入的名称；
  保存前仍可逐项修改。私钥和预共享密钥使用密码框，不进入文件预览或错误文本。
- 按 WireGuard/wg-quick 格式读取 `PrivateKey`、`Address`、`ListenPort`、`MTU`，
  以及每个 `[Peer]` 的 `PublicKey`、`PresharedKey`、`Endpoint`、`AllowedIPs`、
  `PersistentKeepalive`。重复 Address/AllowedIPs 会合并，支持多 Peer、IPv6、
  BOM、CRLF 和注释；省略前缀的 IP 按 `/32` 或 `/128` 导入。
- `ListenPort` 映射为本机 UDP 监听，初始地址 `0.0.0.0`，可在表单改为 IPv6。
  入站开启时显示监听地址和端口；0 表示动态端口。仅出站且无监听的 Peer 必须
  提供远端地址，监听型端点可通过认证握手学习对端地址。
- Address 是分配给本机的隧道 IP/前缀（例如 `10.66.0.2/32`），不是密钥。
  提交时在对应字段显示错误并定位；Rust 同时校验监听、MTU、预共享密钥和保活值。
- 文件里的 `DNS` 不修改客户端 DNS，导入预览和自动填写后明确显示此限制；
  `SaveConfig` 不导入。包含 `Table`、`FwMark` 或 Pre/PostUp/Down 脚本的文件
  拒绝导入，避免把系统 VPN 行为静默改变为端点行为。不会执行文件里的命令。
- 每个文件最多 256 KiB，每批最多 4 MiB，导入后端点总数最多 256。文件在本机
  读取，继续复用全局端点库的版本核对、完整配置预检和保存/应用事务。

格式依据：[wg(8)](https://git.zx2c4.com/wireguard-tools/about/src/man/wg.8)、
[wg-quick(8)](https://git.zx2c4.com/wireguard-tools/about/src/man/wg-quick.8)。
