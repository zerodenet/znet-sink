# v0.0.3 移动客户端基础适配验证记录

2026-10-13。客户端基线 `develop@fd198ba52711f018334860b8a6704546ccd349fd`，
工作分支 `feat/mobile-v0.0.3`，开发版本 `0.0.3-dev.202610131010`。
本次仅在云端实现和验证，没有在用户 Mac 修改、构建或测试。

本记录以交接的完整移动 PRD V3 为基准；HTML 只提供布局参考，没有复制其
模拟数据、事务定时器或假成功。此次是客户端基础适配 draft，**不是完整移动
VPN 交付**。下列“通过”只指对应代码检查或替代 IO 的测试，不代表手机验收。

## 本次代码

- 原生 Android/iOS 或窄窗口采用四固定底栏。高级管理由移动投影保留，不修改
  用户桌面 Lite/Pro 偏好。二级页保存来源，各底栏已访问的页面保留搜索、展开、
  草稿与滚动；工作区、配置和订阅编辑离开时确认未保存内容。
- 概览只有一个首屏统计入口，定位实际全局 Flow。统计工作区复用应用共享会话，
  无损 u64 累计与本次采样分开；当前速率未知显示 `—`，不同实例、配置、epoch
  和观测缺口重新建立基线。后台暂停 UI 查询，保留公共事件通道；前台重新发现。
- 节点页按配置顺序展示全部组，多组展开，搜索不覆盖用户展开记录；区分直接成员
  selector 与自动/其他组。组或单节点测速固定目标，停止保留 draining 事实，
  重试只取原任务失败项。节点、测速与模式调用增加客户端作用域前置条件。
- 配置与订阅为两个分段，路由、规则集和原生工作区保留二级入口。离线本地选中
  与运行态确认分开。沿用 Rust 配置、订阅、DNS、规则、持久化和应用链，未新增
  “移动 JSON schema”。未知字段、来源归属和原有权限仍由现有领域决定。
- 端点只新增受现有能力及 precondition 准入的“清除运行覆盖”。存在客户端覆盖
  时阻止把这个操作当作恢复来源，并提供工作区入口。完整来源恢复仍未完成。
- 统计只呈现所选范围实际提供指标的平面；没有虚构 Host identity。新 Packet
  页面使用 `zero-api` DTO、真实查询与关闭命令、固定目标确认、profile/实例/
  revision 校验、无损计数和回执核对；未知结果不会自动重放。
- 移动 Tauri 装配、私有容器和权限独立于桌面。没有移动运行桥时，拒绝桌面
  进程、代理、TUN、安装器、原始 IPC 和运行操作，不伪造可转发内核实例。
  发行页分别展示默认随包策略与实际缺失的移动内核产物。

## 平台及浏览器边界

Android 最低系统明确为 7.0/API 24，Galaxy S8 仍是必须真机验证的设备。
API 下限、Rust target 编译和 320px 浏览器检查不能替代该设备的包安装及运行。

当前技术基线使用 Tailwind 4，前端构建显式以 Chrome 111/Safari 16.4 为目标。
系统版本与 WebView 引擎版本需分别检查；不声称 Android 7 原始 WebView 已兼容。
[Tailwind 官方兼容基线](https://tailwindcss.com/docs/compatibility)、
[Tauri WebView 说明](https://v2.tauri.app/reference/webview-versions/)。
iOS 产品最低版本仍需结合这项 UI 基线冻结；此次没有 Xcode 主宿主/扩展构建，
没有付费开发者账号、签名凭据、安装或真实 iPhone VPN 验收。

核心仍需 M01–M04：完整 Proxy 生命周期、外部 Android FD/iOS packetFlow、
保护/绑定物理出口与网络变化、实例绑定且鉴权的异步控制及事件桥。
[跨仓库 API 清单与退出测试](../gui/mobile-platform-v0.0.3.md) 已给出 crate/trait 层次，
core 工作树只读且未推送。默认随包策略尚未落实为移动内核构建产物。

## 验证

| 检查 | 本次结果 | 不能据此推断 |
| --- | --- | --- |
| `pnpm verify:frontend` | 全部现有回归、新移动/Packet 回归、类型检查及生产构建通过 | 手机服务与真实内核转发 |
| `pnpm check` | 0 错误、0 警告 | 系统 API/键盘已在手机验收 |
| Rust fmt、workspace check/all-targets/all-features | 通过 | iOS 宿主/扩展链接与签名 |
| Rust workspace/all-features 测试 | 897 passed、0 failed、7 ignored | ignored 的真实二进制/平台测试已运行 |
| Rust workspace/all-targets/all-features clippy | 通过，保留既有及测试代码警告 | 严格 `-D warnings` 已通过 |
| 共享 Rust Android/iOS aarch64 target check | 两个共享 crate 均通过 | Tauri/iOS 主宿主或扩展已构建 |
| Android Rust 主宿主 API24 check | NDK r27d/API24 clang，`--lib --target aarch64-linux-android --locked` 通过；40 项未调用桌面代码等警告 | APK、签名、安装、TUN FD、S8 或 VPN 验收 |
| Chromium 浏览器 | 320px、多组、搜索/滚动恢复、来源返回、未保存确认、未知速率、统计入口及桌面回归 | Android 原始 WebView、WKWebView 或手机系统返回 |
| WebKit 本地运行 | 未运行：当前云端浏览器下载被网络网关拒绝（403） | iOS/WebKit 验收通过 |

浏览器测试以真实生产组件和明确的替代 IO 执行；测试 fixture 不进入产品模块。
实际 Unix IPC 的 Packet 测试证明查询/关闭 DTO 与回执处理，仍不是网络转发真机证据。
详细云端日志保留于 `/tmp/mobile-frontend-verification.log`、
`/tmp/mobile-backend-tests.log`、`/tmp/mobile-cargo-check.log`、`/tmp/mobile-clippy.log`、
`/tmp/mobile-android-host-check.log`、`/tmp/mobile-browser-full.log`。

## PRD 验收状态

| PRD 范围 | 客户端完成部分 | 尚未通过的部分 |
| --- | --- | --- |
| A01–A02 | 四底栏、移动菜单、320px/字号/安全区样式、单个统计入口与返回测试 | 手机安全区、系统返回、软键盘真机 |
| A03–A04 | 明确状态/不可用反馈，模式作用域守卫及确认链复用 | 系统 VPN 授权、接管、恢复、模式真实移动提交 |
| A05–A07 | 20 组浏览、类型区分、局部测速、draining、失败项重试与失效守卫测试 | 原生服务持有任务、进程恢复与真实协议测速 |
| A08–A14 | 两分段、编辑草稿保护、离线材料与运行态区分；规则/DNS/工作区入口和既有领域复用 | 系统文件读取/保存/分享、完整删除替代/停止预览、三视图/设置迁移补齐、全部表单手机验收 |
| A15–A17 | 安全端点运行覆盖清理，统计平面、epoch/u64、现有固定目标清空链 | 完整端点来源恢复、移动 Peer/Host 后端及真实手机统计 |
| A18–A19 | 连接/日志/诊断入口复用；真实 Packet 控制链与分类本地清理 | 全部移动进程生命周期、系统导出分享、调试捕获协调与平台验收 |
| A20–A22 | 固定插件入口与不可用原因，宿主底栏不交给插件 | 移动 VM/SDK IO/来源授权、敏感加载、安装更新卸载与真机隔离 |
| A23–A24 | 两独立版本入口、真实版本读取、绑定策略与缺失产物区别、关于入口 | 内核随包产物、移动重启/更新安装适配、依赖许可及外链全部手机验收 |

没有合并、发布、改分支保护、购买账号或创建签名凭据。
