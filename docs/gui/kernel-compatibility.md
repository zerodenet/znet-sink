# 客户端内核兼容

客户端当前的回归基线覆盖 Zero **v0.0.2** 和 **v0.0.3-dev** 两代内核。
产品版本不作为能力开关：V1 契约范围必须与客户端相交，具体功能由正在运行的
内核能力声明和配置校验决定。切换到旧内核不会删除配置字段或将 WireGuard
替换成 Direct；旧内核无法接受当前配置时，由原有升级预检查或配置事务拒绝操作。

## 本轮适配

| 边界 | v0.0.2 | v0.0.3-dev |
| --- | --- | --- |
| 控制、能力、配置、错误契约 | 使用发布的 V1 范围；缺失范围仍视为未知 | 使用相同 V1 范围 |
| 普通代理节点 | 保持原 server/port 投影 | WireGuard 单 peer 从 endpoint 投影；多 peer 不将第一项当作整个设备 |
| 协议能力 | 保留已有 TCP/UDP 能力；缺失 compiled 显示未知 | 保留 compiled、实验状态、基线、限制，编译成功不等于业务可达 |
| Health | 没有 outbound_devices 时不虚构 peer 状态 | 投影逐 peer 握手、认证数据、解析失败与退化状态 |
| 配置组合 | 未声明的数据平面字段不主动生成 | 保留 auto_outbounds、final_mode、规则 mode、WireGuard peer 等原字段 |
| 通用规则“跟随最终路由” | final_mode 缺失时不注入 mode | 继承 final_mode，避免将 Flow 路径变为 Auto |

隧道设备状态位于“调试 → 能力”的设备状态表。握手成功与近期收到认证数据
各自展示；内核健康、隧道状态和应用可访问性不能互相替代。刷新失败会清除
旧设备快照并显示查询错误。状态投影不包含私钥、预共享密钥或公钥。

## 回归与实核检查

前端策略回归已加入 CI；Rust 状态解析、配置组合测试属于现有 GUI 测试套件。
浏览器回归覆盖旧内核字段缺失、新内核实验协议未编译、peer 状态和刷新失败。

```sh
node --experimental-strip-types --test scripts/test-kernel-compatibility.mjs
cargo test --manifest-path src-tauri/Cargo.toml --locked -p gui --lib compatibility_tests
pnpm exec playwright test kernel-compatibility.spec.ts --project=chromium
```

本机 Unix socket 实核检查脚本创建两个临时内核，不连接当前客户端，不创建
TUN、不改系统代理/DNS、不安装内核，也不访问真实 WireGuard peer。Windows
依赖现有共享解析/配置单元测试；此脚本不提供 Windows named-pipe 实核验收。

```sh
python3 scripts/check-kernel-compatibility.py \
  --kernel-v002 /absolute/path/to/zero-v002 \
  --kernel-v003 /absolute/path/to/zero-v003 \
  --captures /tmp/znet-kernel-compatibility
ZNET_KERNEL_COMPATIBILITY_CAPTURES=/tmp/znet-kernel-compatibility \
  cargo test --manifest-path src-tauri/Cargo.toml --locked -p gui --lib \
  real_kernel_version_matrix -- --ignored
```

脚本验证两代内核的 health/capabilities/runtime/stats/policies/active_flows/
tun_status 查询、只修改内存的配置应用，以及拒绝未知 schema 后 revision 和
原文件不变。另验证新路由字段与 WireGuard 配置在新版接受、旧版拒绝。
这些检查不证明真实 TUN、远端握手、应用访问、网络恢复或跨平台实机通过。

## 本地验证记录（2026-09-28）

| 检查 | 实际结果 |
| --- | --- |
| 两代实核 IPC、配置应用与拒绝回滚 | `0.0.2-dev.202609181034`（de92dc8）与 `0.0.3-dev.202609271529`（5200922）均通过 |
| 新增 Rust 兼容测试 | 8 项通过；另用两份实核响应显式运行版本矩阵，1 项通过 |
| 已有配置、解析、快照集成测试 | 63 项通过 |
| configuration 单元测试 | 29 项通过、1 项依赖额外实核环境的测试未运行；与新增测试有重叠 |
| 前端兼容策略 | 2 项通过 |
| 浏览器 | 3 项通过；390 / 900 / 1280 宽度无页面横向溢出 |
| 静态检查与构建 | `pnpm check` 0 错误、0 警告；`pnpm build`、Rust 格式检查与 diff 空白检查通过 |

浏览器状态来自测试 fixture，实核检查来自上述本机二进制。v0.0.3 二进制
不包含内核对话中尚未发布的网络恢复修改；客户端适配同时参照了最新源码。
此次没有安装或发布客户端安装包，也没有改动正在运行的内核、系统代理或 DNS。

CI 已增加 Linux / macOS Rust 兼容测试，以及 Linux / Windows 前端兼容策略测试。
Windows 内核契约任务同时检查已发布的 v0.0.2 基线和内核 develop。
这些 CI 修改尚未推送执行，Windows / Linux 实机及真实 TUN 访问仍需独立验收。

## 本轮修改文件

- 状态模型与 IPC 解析：`src-tauri/src/models/gui_core.rs`、
  `src-tauri/src/kernel/zero/{parsing,queries,events}.rs`、`src/lib/types/gui-api.ts`。
- 节点投影与路由组合：`src-tauri/src/kernel/zero/config.rs`、
  `src-tauri/src/configuration/rules.rs`。
- 能力判断与界面：`src/lib/services/{kernel-capabilities,kernel-health}.ts`、
  `src/lib/components/tabs/CapabilitiesTab.svelte`。
- Rust 回归：`src-tauri/src/kernel/zero/{compatibility_tests,config_compatibility_tests}.rs`、
  `src-tauri/src/configuration/rules_compatibility_tests.rs`。
- 策略与实核脚本：`scripts/test-kernel-compatibility.mjs`、
  `scripts/check-kernel-compatibility.py`。
- 浏览器回归：`tests/ui-controls/{kernel-capabilities.ts,kernel-compatibility.spec.ts,config.ts,Harness.svelte}`。
- CI 与记录：`.github/workflows/ci.yml`、本文件。

上述清单不包含工作区原有的插件、订阅与版本号修改。
