# 2026-09-05 最低生产可用候选验收

本页保留第一轮验收和已存档的安装包；后续客户端体验改进见[首版体验第二轮](./first-release-polish-20260905.md)。
文中的未提交、未发布状态是当时的快照，后续提交和正式编号见 [RC 构建记录](./rc-20260905.md)。

本次交付是本地候选构建，客户端保留当前版本号 `0.0.16-rc.202609041703`。
首次无代理配置启动需要配套本次修复后的 Zero 内核；已有发布版本
`0.0.16-rc.202609041712` 的原始二进制不支持空配置待命。

## 已落地范围

| 场景 | 行为与验收依据 |
| --- | --- |
| 应用配置保存中断、历史主文件损坏 | 同目录临时文件同步后原子替换；有效旧配置备份；启动从备份恢复并保留损坏副本。覆盖备份失败、缺失主文件、损坏且无备份。 |
| 内核升级失败 | 下载及 CLI 预检查期间保持旧进程；替换前备份二进制、伴随文件、清单和配置；失败时恢复旧文件并尝试恢复运行和捕获状态。文件回退测试覆盖新文件删除、执行权限、回退失败后备份保留。 |
| 启动响应失真 | 健康 IPC、精确子进程 PID、连续健康窗口和超时；真实内核测试覆盖启动、绑定冲突及生命周期 EOF。启用托管系统代理前还检查本地监听。 |
| 连接操作竞争 | 连接、断开和配置变更共用操作锁；状态轮询不撤销启动或升级期间保留的代理。 |
| 首次无代理配置启动 | Zero 空配置保留管理循环，首次 `config.apply` 后启用监听；绑定失败仍可重试，移除最后监听后回到待命。保持现有监听异常退出监督。 |

本次构建包含工作区原有的 TUN CIDR/mask 归一化和 RC 版本保留策略改动，没有提交、发布或覆盖已安装客户端。

## 验证命令

在 GUI 工作区，Node 使用本机的 22.23.0：

```sh
pnpm verify:frontend
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml --locked --all-targets -j 2
cargo test --manifest-path src-tauri/Cargo.toml --locked -j 2
PLAYWRIGHT_CHROMIUM_CHANNEL=chrome pnpm test:ui-browser --project=chromium
ZNET_TEST_ZERO_BINARY=/path/to/zero cargo test --manifest-path src-tauri/Cargo.toml --locked readiness -- --include-ignored
pnpm tauri build --no-sign --bundles app,dmg --config '{"build":{"beforeBuildCommand":""}}'
python3 scripts/test-packaged-lifecycle.py --app-binary '/path/to/ZNet Sink.app/Contents/MacOS/gui' --kernel-binary /path/to/zero
```

打包命令复用 `verify:frontend` 刚构建完成的前端资源。隔离包测试使用临时数据目录、临时内核副本、关闭自动连接和 TUN，检查启动及备份恢复后精确 PID、强制终止 GUI 后内核回收，并比对测试前后的系统代理。

在 Zero 工作区：

```sh
cargo fmt --all -- --check
RUST_MIN_STACK=16777216 cargo test --workspace --no-fail-fast -j 2
cargo build --release -j 2
```

## 实测结果

- 前端完整验证通过，Svelte 检查 0 errors / 0 warnings。
- GUI 后端格式、全目标检查、全量测试通过；仍有 6 条既有平台条件编译警告。
- Chrome 浏览器 8/8 通过。Playwright 指定浏览器版本不支持本机 macOS 13，WebKit 未通过本机验收。
- 对原发布内核的就绪及进程生命周期回归 7/7 通过，包含两个显式启用的真实内核测试。
- macOS x86_64 `.app`、DMG 和 updater tar.gz 构建成功；DMG `hdiutil verify` 通过。
- 原发布内核的首次空配置包测试发现 `NoInbounds` 启动失败，因此交付需包含本次配套内核修复。

- 配套 release 内核与客户端包的隔离测试通过：首次启动、备份恢复、两次强退 GUI 后子进程回收；系统代理前后一致。
- 内核完整回归采用仓库 CI 相同的 16 MiB 测试线程栈。默认线程栈曾在深层 SOCKS5/VLESS UDP 调试测试中溢出，不能把该次运行记为通过；本次只调整测试进程环境，不改变生产线程配置。

内核默认工作区特性全量回归完成：1366 passed、0 failed、79 ignored，命令退出码 0。新增空配置 IPC 全流程、首次绑定失败后的重试、应用监督错误上抛均通过；没有把该结果扩大为全部 feature 组合或真实特权网络验收。

## 发布边界

- 构建未签名、未公证，未上传发布，updater tar.gz 不能直接作为正式自动更新发布物。
- 实机范围为 macOS Intel；Windows、Linux、Apple Silicon 及真实授权 TUN/系统代理切换未在本次验收中覆盖。
- 正常升级错误自动回退；升级过程中被强制终止或断电后的 `pending` 备份暂需人工恢复，没有启动时自动续回退。
- 备份暂不自动轮转。旧客户端遗留且不属于当前实例的内核没有被接管或清理。
- 所有进程强退测试只作用于本次隔离测试启动的 GUI/内核，正在使用的安装实例保持运行。


## 候选文件

- [客户端 DMG](</Volumes/tool/rust/zero/target/production-candidate-20260905/first-client/ZNet Sink_0.0.16-rc.202609041703_x64.dmg>)
- [配套内核归档](/Volumes/tool/rust/zero/target/production-candidate-20260905/zero-darwin-x86_64.tar.gz)
- [构建清单及 SHA256](/Volumes/tool/rust/zero/target/production-candidate-20260905/manifest.json)

客户端与内核版本号均沿用工作区现有值，构建清单中的 SHA256 区分本地候选与原发布二进制。内核归档是单独交付物，DMG 没有内置这份新内核。当前安装实例没有自动替换；升级候选验证应使用上面的配套归档。
