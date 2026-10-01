# 全局出口与全部节点客户端核对

日期：2026-09-30。只调整 `/Volumes/tool/rust/gui`；未修改、启动或操控真实内核，
未更改运行中的系统代理/TUN。客户端产品版本保持 v0.0.2。

## 原实现的问题

1. 汇总页按策略组顺序分配唯一节点，重复成员后续不显示，造成部分组消失、
   左右计数含义不同。它不是全部策略组的完整展开。
2. 汇总页点击节点寻找首个 selector 调用 policies.select；无法表达独立全局出口。
3. 原生 gui_set_proxy_mode 已支持 globalOutbound，但前端调用未传目标，状态映射
   也未保留 globalOutbound，无法可靠显示/确认实际指定的出口。
4. 概览使用规则兜底或首个策略组呈现，可能与全局目标不同。

## 当前行为

- 全局模式初始及切入时默认打开“全局出口”，平铺所有配置出站与节点组。
  未被其他组引用的组也显示；每个目标出现一次，保留真实组类型标签。
- 新全局目标允许节点或非 selector 组。selector/select 显示但不可在此新选，
  在左侧独立管理。已有配置的 selector 全局目标保持原值，不自动改写。
- UI 与原生客户端边界检查目标是否存在、组是否为空、全部引用链是否有循环。
  不创建虚拟 selector、不添加组成员；最终配置和运行校验仍由内核确认应用路径负责。
- 选择全局目标使用已有模式接口写入顶层 mode.outbound，确认回读目标后更新选择。
  不调用 policies.select，不改 route.rules、route.final 或 outbound_groups。
  切回规则模式使用原规则兜底，并返回明确的策略组视图。
- 规则/直连模式的“全部节点”平铺去重后的具体出站，组在左侧独立展示。
  汇总用于浏览/搜索/测速，禁止猜测某个父组进行手动选择。
  进入具体 selector 后仅切换该组的直接成员，自动组不提供成员手动选择。
- 专业概览显示指定全局目标及其已确认的组路径，入口打开节点页选择全局出口。
  简约概览也从全局目标解析；目标缺失时不代用第一个策略组。
  未被选作全局目标的分流组失败不再作为“已选出口失败”提示。

## 修改入口

- `NodesTab.svelte`、`NodesGroupSidebar.svelte`、`nodes-inventory.ts`、
  `NodesGridCard.svelte`、`NodesListRow.svelte`：视图、选择范围和组类型。
- `nodes-view-model.ts`：移除过时的首组归属折叠逻辑。
- `services/core.ts`、`services/gui-state.svelte.ts`、`types/gui-api.ts`：传入、映射并确认目标。
- `src-tauri/src/services/proxy_mode.rs`、`proxy_mode/target.rs`：当前配置下的目标预检。
- `OverviewTab.svelte`、`overview/model.ts`、`ProfessionalOverview.svelte`：概览出口对齐。
- 节点/概览脚本、模式 Rust 测试、浏览器 fixtures 与 `nodes-global.spec.ts`：回归证据。

## 本地验证与限制

- Rust `--lib proxy_mode`：13 项通过，含新增目标校验、原规则保留、端点出站角色测试。
- `pnpm test:nodes`：视图、测速状态与选择参数脚本通过。
- `pnpm test:overview`：概览模型 24 项、命令状态 20 项通过。
- Chromium：全局/规则视图 4 项、既有 WireGuard 3 项、紧凑布局 3 项、
  概览全局/嵌套策略 2 项通过。
- `pnpm check`：0 错误、0 警告；前端生产构建、Rust 格式与 diff 检查通过。

浏览器使用内存 fixture，Rust 测试检查真实客户端的配置转换；以上不证明真实
全局网络转发已经实机验收。未安装客户端、构建安装包、提交推送或发布版本。
真实内核全局模式的数据面转发、WireGuard 出口及 Windows/Linux 原生运行仍需实机验收。
