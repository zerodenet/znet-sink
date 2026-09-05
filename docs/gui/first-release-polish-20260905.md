# 2026-09-05 首版体验第二轮

本轮在[第一轮候选](./production-candidate-20260905.md)基础上收口客户端版本管理流程，配套 Zero 内核没有新增改动。保留工作区现有版本号，以构建清单 SHA256 区分候选。

## 用户可见变化

- 更新提示按语义版本排序和比较，较新的 RC 不会被旧稳定版提示升级；没有稳定版更新时明确显示“暂无稳定版更新”。
- 下载完成后继续显示校验、备份、安装、启动恢复或回退阶段，直到后端事务完成。进行中阻止重复安装和关闭版本窗口。
- 安装失败的完整消息保留在窗口中，可直接重试；版本列表获取失败显示错误和刷新入口，不再误显示“该渠道暂无可用版本”。
- 安装成功后只刷新前端状态，去掉第二次保存内核路径引起的多余运行时切换。
- 同版本重装也修复 Unix 执行权限；回退恢复原权限，即使原文件与候选文件内容相同。

## 回归证据

- `pnpm verify:frontend` 通过，包含新增版本策略回归；Svelte 检查 0 errors / 0 warnings，生产前端构建成功。
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 通过。
- `cargo test --manifest-path src-tauri/Cargo.toml --locked -j 2`：435 passed、0 failed、4 ignored（含一个文档测试）。保留 6 条既有平台编译警告。
- `PLAYWRIGHT_CHROMIUM_CHANNEL=chrome pnpm test:ui-browser --project=chromium`：12/12 通过。新增 4 条版本管理浏览器测试使用本地夹具，不执行真实下载或内核升级；覆盖进度、回退错误保留、重复配置写入和列表失败。

- `pnpm tauri build --no-sign --bundles app,dmg` 构建成功（通过配置跳过重复前端构建，复用刚验证的资源），`hdiutil verify` 通过。
- 新包隔离测试通过：首次启动及备份恢复都获得精确子进程 PID 和健康 IPC，两次强退 GUI 后内核均退出；测试前后系统代理一致。

## 候选文件

- [第二轮客户端 DMG](</Volumes/tool/rust/gui/src-tauri/target/production-candidate-20260905-polish/ZNet Sink_0.0.16-rc.202609041703_x64.dmg>)
- [配套内核归档](/Volumes/tool/rust/zero/target/production-candidate-20260905/zero-darwin-x86_64.tar.gz)
- [构建清单与 SHA256](/Volumes/tool/rust/gui/src-tauri/target/production-candidate-20260905-polish/manifest.json)
- [包启动验收日志](/Volumes/tool/rust/gui/src-tauri/target/production-candidate-20260905-polish/smoke.log)

安装包已另存至候选目录，后续常规构建不会覆盖本轮文件。第一轮 DMG 也保留在其原验收记录的存档位置。

## 交付边界

本轮未覆盖已安装客户端、未提交或上传发布。候选为未签名的 macOS Intel 构建；WebKit、其他操作系统、真实特权 TUN 和系统代理切换仍以第一轮记录的边界为准。升级中强制退出或断电后的 pending 备份仍需人工恢复，本轮未实现启动时自动回退。内核归档单独交付，DMG 不包含这份配套内核。
