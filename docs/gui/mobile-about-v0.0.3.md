# 移动端关于与许可资料

`MobileAbout.svelte` 是原生移动宿主专用页面。桌面保留现有 AboutPanel；移动页面不引用 desktop updater，也不显示桌面下载、安装或任意文件执行控制。

## 信息与平台动作

`services/mobile-about.ts` 使用 Tauri 实际 `getName/getVersion/getIdentifier` 读取已安装应用信息。读取分别确认，单个失败不丢弃其他有效字段；失败、空值及畸形返回显示 `—`，不回落为源码中的演示版本。项目声明标识为 `org.zerodenet.znetsink`、作者为 ZeroDenet；运行标识不一致会保留真实返回值并提示核对安装来源。

当前 native bridge 尚未提供真实运行 Proxy 或随包运行产物的 authoritative 版本回执，两个 Zero 版本均为 `—` 并说明原因。不会拿配置验证 crate 的版本、客户端包版本或桌面下载缓存版本冒充运行内核版本。

官网、仓库与 GitHub issues 使用固定项目地址。仅在用户点击时调用现有 `platform_open_url`，交给系统浏览器；初始化与信息刷新只读 app metadata 与本次构建许可目录。打开失败保留准确地址，用户再次明确点击复制才调用现有 `platform_clipboard_write` 及其既有安全 fallback，复制失败不显示成功。问题反馈不构造带日志/秘密的 query，不自动提交 issue、不上传诊断材料。准备脱敏诊断材料仅导航到诊断页面，由后续用户操作决定保存或分享。

## 构建许可目录

项目 MPL-2.0 全文仍直接来自仓库 `LICENSE`。依赖目录改为消费真实构建生成的 JSON：前端 Vite 模块/产物证据、目标 Rust 解析与可选编译回执、Android 固定随包 asset 或 iOS 无签名 SDK 收集结果。宿主 `app_license_inventory` 仅只读已有清单，主窗口校验及 blocking worker 不申请文件、VPN 或插件权限；Android reader 不接收路径/URI。缺失原生清单、构建 stub、正文或声明分别保留原因。

页面提供名称/许可/来源搜索、生态筛选、各依赖与正文折叠，显示版本、target/profile、材料 SHA-256、来源、各类证据与缺口。解析图、构建工具和系统框架不会被称作最终链接保留的运行依赖。原文按纯文本显示；许可证标识不替代正文，SPDX 元数据不当作全文。具体生成接口、严格发行验证和材料来源见 [构建依赖许可目录](dependency-license-inventory-v0.0.3.md)。

## 验证

- `scripts/test-mobile-about.mjs` 使用实际 Tauri app API 与平台 gateway，断言 metadata invokes、无自动外部操作、仅点击反馈时打开固定 issues URL、浏览器失败后单独复制、错误/畸形 metadata 为未知、重复动作准入、卸载后晚到结果失效和构建清单的损坏引用/未提供材料状态。
- `tests/ui-controls/mobile-about.spec.ts` 的独立 Chromium 页面在 320 px 验证实际组件、44 px 按钮、无横向溢出、MPL 全文末尾附录可访问、反馈与复制实际平台 invoke 参数。测试中拦截只读许可 JSON 与 IPC，不执行真实设备浏览器/剪贴板。
- `pnpm check` 验证 Svelte/TypeScript；共享 UI 控件契约检查涵盖新组件。

Android/iOS 真机系统浏览器与剪贴板、系统字号/安全区以及发行目标全部依赖法律资料的完整性尚未验收。页面和浏览器测试不替代这些平台结果。
