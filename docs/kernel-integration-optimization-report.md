# 内核接入优化报告

生成时间：2026-06-08  
目标：让当前 Tauri/SvelteKit GUI 更稳定地消费 Zero 最新控制面契约，并降低后续协议能力、配置编辑和诊断功能的实现成本。

## P0：先修契约正确性

### 0. 将“GUI 托管内核常驻”作为默认生命周期

新的产品原则是：GUI 启动即检查并托管内核；GUI 运行期间内核默认常驻；GUI 退出时托管内核退出；用户界面不提供普通关闭内核，只提供重启内核。

建议调整：

- `AppCoreConfig.auto_start` 默认改为 `true`，或保留字段但不再影响默认托管启动。
- Tauri `setup` 阶段总是执行内核可用性检查。
- 如果已有托管内核运行，复用并对账 health/capabilities。
- 如果内核不可用，尝试启动托管内核。
- UI 中“停止内核”类入口改为“重启内核”或诊断入口，不作为常规业务操作。
- 托盘退出和应用退出必须停止托管内核；关闭窗口仍可隐藏，不等于退出应用。

这应作为所有优化项的前置生命周期约束。

### 1. 按请求类型统一解包 IPC QueryResponse

当前 `query_result()` 只剥离 `ApiResponse.result`，但最新文档规定 IPC Query 的 `result` 还会按变体名包裹，例如 `{"runtime":{...}}`。建议增加 request-aware helper：

- `query_result_variant(request, "runtime", options)` 返回 `result.runtime`
- 如果返回值没有该 key，再兼容旧形态直接返回 `result`
- 为 `health`、`capabilities`、`active_flows`、`flow`、`tun_status` 增加单元测试

收益：消除 health 版本号丢失、capabilities 为空、连接详情失败、TUN 状态误判等连锁问题。

### 2. 明确 socket 路径策略

建议拆成两个概念：

- 外部默认 Zero daemon：遵循文档，Unix 使用 `~/.zero/control.sock`，Windows 使用 `\\.\pipe\zero-control`
- GUI 托管 Zero：允许继续使用私有 socket，但必须显式传 `--control-socket`，并在 UI/文档里标记为托管路径

这样既不破坏 GUI 自己启动进程的可控性，也能对齐用户手动启动内核的默认行为。

### 3. 把启动快照集中到一个后端流程

文档推荐的首屏顺序是 health、capabilities、config、runtime、subscribe。建议将 `gui_core_overview` 扩成真正的启动快照：

- `health`：进程存活和 build id
- `capabilities`：功能和权限
- `config`：监听、出站、策略、规则数量
- `runtime`：stats、active/recent flows、日志状态
- `subscribe`：事件增量

前端只消费该聚合 DTO，避免当前 overview、overviewData、coreEvents 各自补数据造成状态分散。

## P1：补齐 GUI 高价值能力

### 3.5 支持无配置启动与 9090 Web API 兜底

当前 `core_process::start` 要求 `config_path`，这与“GUI 默认保持内核常驻”冲突。建议分两级处理：

- 最佳方案：内核支持无配置启动，进入 `waiting_for_config` 或类似健康状态，并开启本地控制面。
- 兼容方案：GUI 在无 active 配置时生成最小临时配置，仅启用 `api.control`：

```json
{
  "inbounds": [],
  "outbounds": [],
  "outbound_groups": [],
  "runtime": {},
  "api": {
    "control": {
      "enabled": true,
      "listen": { "address": "127.0.0.1", "port": 9090 }
    }
  },
  "mode": { "type": "rule" },
  "route": { "rules": [], "final": { "type": "direct" } }
}
```

启动后 GUI 应把状态展示为“内核已运行，缺少代理配置”，而不是“内核未启动”。有配置文件时则导出 active 配置并保持常驻。

### 4. 扩展命令覆盖面

优先新增：

- `config.apply`：配置热加载
- `mode.set`：模式热切换
- `diagnostics.dns_lookup`：DNS 调试
- `diagnostics.trace_route`：路由解释
- `policies.probe` 的 GUI 业务接口：触发 url_test 探测

建议保留现有重启式模式切换作为 fallback：当 `capabilities` 未声明 `mode.set` 或 `config.apply` 时再重启内核。

### 5. 补齐查询覆盖面

优先新增：

- `recent_flows`：连接页展示近期完成连接
- `sinks`：事件投递页和诊断页
- `diagnostics`：诊断概览
- `tun_status`：去掉非文档命令 `tun.status` fallback，或仅作为旧版本兼容路径

连接页建议同时消费 `active_flows` 和 `recent_flows`，事件流只做增量更新，断流后用 query 重建。

### 6. 能力矩阵建模

将 `GuiZeroCapabilities` 扩展为：

- `protocols: ProtocolCapability[]`
- `buildFeatures: string[]`
- `experimentalFeatures?: string[]`
- `permissions`
- `adapters`
- `sinks`

前端能力页不要只看 `features`，要按 `protocols[*].inbound/outbound`、`status`、`limitations` 判断协议是否可配置、是否只读、是否提示实验性。

## P2：降低运维和交互风险

### 6.5 区分重启内核和关闭应用

在新模式下，“内核关闭”只应出现在两种场景：

- GUI 退出：清理托管内核和系统代理。
- 用户选择“重启内核”：先停止旧进程，再按当前配置重新启动。

不建议继续把 `core_process_stop` 暴露为普通业务按钮。底层命令可以保留给专业诊断，但业务 UI 应隐藏或改名，避免用户把 GUI 运行中的必要下层服务关掉。

### 7. 配置编辑采用草稿模型

推荐流程：

1. 从 `config` query 获取权威快照。
2. 前端维护本地草稿，不直接修改运行态。
3. 保存前调用 `config.validate`。
4. 成功后调用 `config.apply`。
5. 再查 `config` 和 `runtime` 对账。

如果后续内核提供 `config.plan_apply`，GUI 可在保存前展示“可热加载/需重启”的影响范围。

### 8. 事件流增加自动重连

当前订阅断开后主要标记状态。建议按文档加入：

- Zero 未启动：1 到 5 秒退避重连
- Zero 重启：关闭旧连接，延迟后重连
- 重连成功后补拉 `runtime` / `stats` / `policies`
- 对未知事件保留原始 JSON 到诊断日志，但不污染业务 UI

IPC 不支持 `since` 回放，因此重连后必须靠快照对账。

### 9. 模式切换从重启迁移到热切换

最新内核支持 `mode.set`，GUI 当前通过修改 `route.mode`、导出配置和重启实现。建议迁移：

- 运行中：优先 `mode.set`
- 未运行：修改顶层 `mode` 并导出配置
- 不支持 `mode.set`：fallback 到现有重启流程

收益：降低连接中断、提升模式切换响应速度，并对齐内核配置模型。

### 10. 更新项目文档和测试

建议补充测试：

- IPC QueryResponse externally-tagged 解包
- Unix/Windows endpoint 解析
- subscribe 确认帧和事件帧区分
- `active_flows`、`recent_flows`、`tun_status` 解析
- `capabilities.protocols` 序列化到前端类型
- 模式切换：`mode.set` 成功、fallback 重启两条路径

文档需要同步更新 `docs/gui/core.md`、`docs/gui/zero-adapter.md` 和能力页说明，避免继续引用旧 socket 与旧模式模型。

## 推荐迭代切分

1. 生命周期调整：GUI 启动默认托管内核常驻，GUI 退出清理托管内核，业务 UI 只保留重启。
2. 无配置启动：优先接入内核无配置提示；短期用最小临时配置 + `127.0.0.1:9090` Web API 兜底。
3. 契约修复：QueryResponse 解包、连接/TUN/capabilities 解析测试。
4. Endpoint 修复：默认路径对齐和托管路径说明。
5. 能力页升级：协议矩阵 DTO 和前端展示。
6. 控制命令补齐：`config.apply`、`mode.set`、DNS/路由诊断。
7. 配置编辑闭环：validate/apply/对账。
8. 事件可靠性：自动重连和快照补偿。
