# 移动端关于与许可资料

`MobileAbout.svelte` 是原生移动宿主专用页面。桌面保留现有 AboutPanel；移动页面不引用 desktop updater，也不显示桌面下载、安装或任意文件执行控制。

## 信息与平台动作

`services/mobile-about.ts` 使用 Tauri 实际 `getName/getVersion/getIdentifier` 读取已安装应用信息。读取分别确认，单个失败不丢弃其他有效字段；失败、空值及畸形返回显示 `—`，不回落为源码中的演示版本。项目声明标识为 `org.zerodenet.znetsink`、作者为 ZeroDenet；运行标识不一致会保留真实返回值并提示核对安装来源。

当前 native bridge 尚未提供真实运行 Proxy 或随包运行产物的 authoritative 版本回执，两个 Zero 版本均为 `—` 并说明原因。不会拿配置验证 crate 的版本、客户端包版本或桌面下载缓存版本冒充运行内核版本。

官网、仓库与 GitHub issues 使用固定项目地址。仅在用户点击时调用现有 `platform_open_url`，交给系统浏览器；初始化与信息刷新只读 app metadata。打开失败保留准确地址，用户再次明确点击复制才调用现有 `platform_clipboard_write` 及其既有安全 fallback，复制失败不显示成功。问题反馈不构造带日志/秘密的 query，不自动提交 issue、不上传诊断材料。准备脱敏诊断材料仅导航到诊断页面，由后续用户操作决定保存或分享。

## 已提供许可文本与可追溯来源

`services/mobile-license-notices.ts` 在前端编译时直接 raw 导入仓库 `LICENSE`（MPL-2.0），完整显示所有正文与附录。以下资料也以 raw 导入保留原文，不翻译或改写许可：

| 材料 | 版本和许可标识来源 | 正文来源 | 使用依据 |
| --- | --- | --- | --- |
| Lucide Svelte 图标 | 本次安装的 `@lucide/svelte/package.json` | 同包 `LICENSE` | 移动导航及共享 UI 实际导入该包 |
| Inter 字体 | 本次安装的 `@fontsource-variable/inter/package.json` | 同包 `LICENSE` | `src/app.css` 导入 `wght.css` |
| flag-icons 资源 | 本次安装的 `flag-icons/package.json` | 同包 `LICENSE` | `src/app.css` 导入旗帜样式 |
| 项目随附规则来源 | `src-tauri/resources/builtin-rules/manifest.json` 的 sourceCommit/sourceLicense | 同目录 `LICENSE.meta-rules-dat` 与 `NOTICE.md` | 随附规则资源及其来源记录；不据此承诺每个目标已运行这些规则 |

版本与许可标识不硬编码为审计时的值，后续安装版本变更会进入同一次构建的 About 资料。许可文件缺失导致构建失败，不静默替换为空文本。本集合是明确列出的有限许可资料，不是全部实际运行依赖清单。

## 完整 actual-build 目录仍待接入

现有前端构建流程没有收集最终 Rollup 模块到 npm package / asset 的归属、完整传递依赖许可文件与 notices。`pnpm-lock.yaml` 只能证明解析集合，不能证明哪些文件进入最终应用：devDependencies 中亦有真实运行 UI、字体与图标，不能简单按 dependency 类别删掉。

现有 Rust `src-tauri/build.rs` 校验 product feature 与前端 composition 并调用 tauri_build，没有生成目标、启用 features、实际编译依赖及每包 license files 的目录。Cargo.lock 同样包含多 target/feature 解析材料，不能被称作设备实际依赖列表；crate metadata 中的 SPDX 表达式也不能代替全文。Android JNI/系统组件、iOS frameworks、Tauri plugins 与随包资源还需要发行目标的产物/来源记录。

可靠扩展应由发行构建收集实际 frontend module/asset 归属、选定 target/features 的 Rust 编译/metadata 材料及 native resources，保留本地 package LICENSE/NOTICE 原文与 provenance，并标注未提供文件或未确认归属。本次没有增加未经接入的生成器，也没有猜测第三方许可证。页面明确说明其余 npm、Rust 和平台原生依赖目录未完整提供；不得把 A24 的完整依赖许可交付标为全部完成。

## 验证

- `scripts/test-mobile-about.mjs` 使用实际 Tauri app API 与平台 gateway，断言 metadata invokes、无自动外部操作、仅点击反馈时打开固定 issues URL、浏览器失败后单独复制、错误/畸形 metadata 为未知、重复动作准入、卸载后晚到结果失效和真实许可文件导入。
- `tests/ui-controls/mobile-about.spec.ts` 的独立 Chromium 页面在 320 px 验证实际组件、44 px 按钮、无横向溢出、MPL 全文末尾附录可访问、反馈与复制实际平台 invoke 参数。测试中仅拦截 IPC，不执行真实设备浏览器/剪贴板。
- `pnpm check` 验证 Svelte/TypeScript；共享 UI 控件契约检查涵盖新组件。

Android/iOS 真机系统浏览器与剪贴板、系统字号/安全区以及完整发行依赖许可目录尚未验收。页面和浏览器测试不替代这些平台结果。
