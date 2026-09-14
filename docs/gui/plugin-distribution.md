> 2026-09-14 更新：插件现为精简/专业模式共用的顶层菜单。默认“发现插件”读取固定中央目录，用户选择登记仓库的发布版本并在线下载安装；“已安装”管理权限与运行，“从本地安装”保留为辅助入口。两种安装复用签名、设备兼容、版本与授权撤销校验，在线下载通过客户端网络权限执行器。尚无自动升级或回滚按钮。中央目录当前为空，线上真实插件下载安装尚未验收。以下保留早期实施历史。

# 插件注册、发布与本地安装链

2026-09-12。接续 [沙箱基础](./plugin-foundation.md)。本批交付独立 Rust 模块与 `znet-plugin` 命令行，覆盖中心读取 → 作者发布发现 → 签名验证 → 本地安装/升级/回滚/卸载。生产 GUI、系统沙箱与移动端执行尚未接入；安装不代表启用。

## 三方职责

- 中心 `zerodenet/plugins` 保持目录 v2：注册 host、ID、发布者公钥、仓库、信息和能力上限。不保存版本、安装包、设备兼容范围或发布历史。
- 作者仓库通过 GitHub Releases 发布 `.zspkg` 包和注册条目指定的 metadata_asset（通常 `marketplace-entry.json`）。日常发布不修改中心。
- 客户端从固定中央 HTTPS 地址读取 `catalogs/znet-sink.json`，从登记仓库发现 Releases，验证作者签名和目录能力上限，检查使用端，在自己的存储目录安装。

这与 ZBoard 共用目录角色，但客户端包有独立格式和执行契约。注册到 `zboard` 不会自动注册到 `znet-sink`，二者不共享权限。

## 使用端契约：已实现

组件 manifest 的 `host` 仍为 `znet-sink`，使用端的 `targets` 支持两种形式：

```json
"targets": "any"
```

```json
"targets": [{"os":"android","arch":"aarch64","device":"phone"}]
```

`any` 仅取消设备限制，不豁免宿主版本、运行时 API、能力授权和最低隔离要求。缺失、null、空列表、`all` 等未定义字符串拒绝。明确列表匹配完整 OS/架构/设备形态组合。安装至少要求有一个组件适配当前设备与宿主版本；执行前还必须按实际执行域检查隔离。安装检查不会虚报已有 process 沙箱。

一个包最多八个不同 ID 的组件，各自声明设备与能力；当前没有组件依赖或“整个插件必需组件”的发布语义。组件不可用时不能跨设备借用另一组件授权。当前命令行 pack 一次生成单组件包，Rust 包模型支持多组件。

## 签名包 v1：已实现

扩展名 `.zspkg`，内容是有界 JSON，不是压缩包；没有解压、任意路径、符号链接或原生动态库入口。现阶段只装载 JS 组件资源，不提供任意前端页面/原生二进制资源。

外层字段固定为 `format: "znet-sink.plugin-package.v1"`、`payload`、`signature`。后两者为标准 Base64。Ed25519 签名消息为 ASCII 域 `znet-sink.plugin-package.v1`、一个零字节、payload 解码后的**原始字节**，不重新序列化后验签。公钥只使用中央登记值；包不携带可自我授权的公钥。

payload 固定包含 `schema_version: 1`、`host`、`plugin_id`、`version`、`components`。每个组件包含 `manifest` 与 `source`。验证所有组件 ID 唯一、插件 ID/版本一致、源码摘要和 manifest 契约合法、声明能力不超过中央上限。当前无 UI surface 声明能力；未知字段拒绝。

限制：包 4 MiB、解码 payload 2 MiB、组件最多 8 个，单源码 256 KiB、单 manifest 16 KiB。SHA-256 负责发布资源完整性，Ed25519 负责发布者身份；只检查 SHA-256 不足以安装。

作者发布元数据格式为：

```json
{
  "schema_version": 1,
  "host": "znet-sink",
  "plugin_id": "org.example.plugin",
  "version": "1.0.0",
  "asset": "plugin.zspkg",
  "sha256": "包原始字节的 SHA-256 小写十六进制"
}
```

元数据不是信任根。下载后仍核对包摘要、签名、签名内容中的 ID/host/version 和所有组件声明。签名正确但能力超出登记上限仍拒绝。

## 客户端命令：已实现

在 GUI 仓库根目录执行 `cargo build --manifest-path src-tauri/Cargo.toml -p znet-plugin-sandbox --bin znet-plugin --locked`，以下用 `src-tauri/target/debug/znet-plugin` 运行：

```sh
znet-plugin catalog
znet-plugin releases PLUGIN_ID
znet-plugin install STORE_DIR PLUGIN_ID RELEASE_TAG
znet-plugin list STORE_DIR
znet-plugin inspect STORE_DIR PLUGIN_ID
znet-plugin rollback STORE_DIR PLUGIN_ID
znet-plugin remove STORE_DIR PLUGIN_ID
znet-plugin pack MANIFEST SOURCE PRIVATE_SEED_FILE PACKAGE_OUT METADATA_OUT
```

`pack` 使用作者提供的 32 字节原始 Ed25519 私钥 seed 文件，输出新包和元数据，并打印用于注册的公钥；不打印私钥，不覆盖已有输出。作者需自行安全生成/保存密钥，再把公钥和仓库按既有中心流程注册，上传包与元数据到自己的 Release。此命令不自动注册、上传或发布。

发现最多读取最近 100 个 published Releases，包括预发布；旧版可按精确 tag 获取，不宣称列出了全部历史。草稿拒绝。资源必须来自登记的 GitHub 仓库；下载仅允许 HTTPS GitHub/API/githubusercontent 域及有界重定向，具有连接/总时间、响应字节限制。不提供任意下载 URL 或外部仓库替代入口。

`install` 初次安装或升级到更高版本，拒绝同版本覆盖及隐式降级。`rollback` 是显式选择上一个版本。inspect/安装/回滚都重新读取中心和验签，网络不可用不会悄悄使用过期注册信息；卸载不依赖网络。中心撤回后已保存包不会被 inspect 认可，但**当前没有后台撤回监听或生产运行进程终止机制**。

## 存储与授权边界

独立 STORE_DIR 使用一个带进程文件锁的有界安装状态文件，临时文件写完并 sync 后原子替换。只保留当前版和上一版，无无限历史；最多 16 个插件、状态文件总计 16 MiB。拒绝存储目录/状态/锁文件的符号链接，不防御具有同等本地权限的进程进行任意目录替换。

安装状态只保存签名包，不保存授权、密钥或自动运行标记。失败校验不替换现有安装；升级后组件摘要变化，旧 Authority 无法授权新组件；进程重启也不会恢复旧授权。当前目录公钥轮换或能力缩减使旧版本无法验证时，需显式卸载后重新安装，尚无保留旧信任链的迁移流程。

执行仍使用基础阶段的默认拒绝 Authority。测试证明：签名包安装后直接执行被拒绝，显式授予声明范围后才能调用能力。当前 CLI **不提供已安装第三方包的生产运行命令**；原 `znet-plugin-lab` 仍仅为主动选择本地源码的开发实验器。统一授权、撤销已运行任务、组件异常回收和 UI 结果失效必须由客户端统一管理路径完成；独立 worker 不作为当前前置条件，不能从此处的文件锁推导这些能力已实现。

## 验证与剩余工作

定向测试覆盖 any/明确设备范围、错误设备形态、版本及隔离拒绝、错误公钥、篡改签名内容、冒用插件 ID、能力超限、发布摘要/版本不符、安装升级、旧授权拒绝新组件、重启读取、回滚卸载、失败安装保持当前版、撤回注册查找拒绝、安装文件被篡改、存储符号链接拒绝，以及真实打包 CLI 的签名输出。

2026-09-12 本地结果：21 项测试通过（既有 13 项，加本批 8 项），定向 Clippy `-D warnings`、workspace `cargo check --locked`、workspace rustfmt 检查通过。上一阶段全量回归中未修改的 IPC 就绪测试失败仍未修复，本批不重复宣称全量测试通过。

本机使用真实 `catalog` 命令读取中央目录成功，目录仍为空。本地样例验证不等于线上插件发布、线上下载安装验收或全平台运行。

下一批按 [客户端统一入口、管理与执行管道](./unified-capability-plan.md) 先固定客户端统一入口与管理契约，按能力迁移内置调用并接入插件 SDK。ZBoard 登录、公告与订阅流程由插件实现，不成为宿主业务权限。采用客户端统一管理的独立 VM，独立 worker/OS 沙箱不再是当前前置条件。已安装插件仍不能因安装成功自动运行或取得秘密能力。
