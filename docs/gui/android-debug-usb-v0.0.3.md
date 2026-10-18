# Android Debug 安装与 USB 调试

本指南对应 Draft PR #46 的开发宿主。Debug 可安装、可通过 Chrome 检查 WebView；
它不含已接通的移动 VPN/内核运行服务。Android 最低 API24、arm64；Galaxy S8 的
真实安装、键盘、返回、前后台和网络验收仍需设备执行。云端没有模拟器、AVD 或真机。

## 构建与产物

准备 JDK17/21、SDK36、build-tools35.0.0、NDK27.3.13750724、Rust
`aarch64-linux-android` 和锁定的 pnpm 依赖，然后执行：

```sh
bash scripts/build-android-debug.sh
```

脚本拒绝复用已有证据目录；再次运行前先自行归档/移走上一目录，避免失败时混入旧成功收据。

脚本先构建并嵌入静态前端，覆盖 `devUrl=null`，无需局域网开发服务器。Tauri/JNI
namespace 保持 `org.zerodenet.znetsink`；Android 安装 ID 是
`org.zerodenet.znetsink.debug`，显示名称为 ZNet Sink Debug。不要混用这两个身份。
Debug 开启 inspector，Release 显式关闭，Release 继续使用未签名独立检查链。

`android-debug-host-evidence/` 包含 APK、`SHA256SUMS`、实际 apksigner/aapt/ZIP/ELF/DEX
收据、许可清单与测试日志。收据的 commit 只表示构建源码身份；设备执行字段保持 false。
嵌入资产键是静态包证据，不能替代启动和页面渲染检查。Actions 下载的是证据 ZIP，
ZIP 的 digest 与内部 APK 的 SHA-256 是两个不同值。解压后先执行：

```sh
sha256sum -c SHA256SUMS
```

每次构建由 SDK 在私有临时目录生成开发密钥，退出时清理；不上传、不缓存或保留密钥。
这是开发签名，不是正式签名、发布或商店资格。[Android 签名说明](https://developer.android.com/studio/publish/app-signing)。
不同构建的证书可能不同，不能承诺下一 APK 可覆盖前一次 Debug 安装。

## 用户在设备上授权后连接

由设备所有者手动开启开发者选项和 USB 调试、解锁手机并核对电脑 RSA 提示。
本项目不替用户修改安全设置、接受信任或启用无线调试。电脑安装官方
[SDK Platform Tools](https://developer.android.com/tools/releases/platform-tools)；Windows
可能需要设备厂商 USB 驱动。连接后遵循 [ADB 授权说明](https://developer.android.com/tools/adb#Enabling)：

```sh
adb devices -l
adb -s SERIAL install -r app-universal-debug.apk
adb -s SERIAL shell am start -n org.zerodenet.znetsink.debug/org.zerodenet.znetsink.MainActivity
```

将 SERIAL 替换为已确认的目标序列号；unauthorized 状态先在手机处理授权。安装失败
保留错误原文，不自动重试卸载或清数据。若出现 `INSTALL_FAILED_UPDATE_INCOMPATIBLE`，
通常是临时开发证书变化；设备所有者可自行决定卸载**仅 Debug 应用**后重装，卸载会
丢失该 Debug 应用的本地数据。不要卸载普通包、自动 `pm clear` 或降低设备保护。

记录设备事实：

```sh
adb -s SERIAL shell getprop ro.build.version.sdk
adb -s SERIAL shell getprop ro.product.cpu.abi
adb -s SERIAL shell dumpsys webviewupdate
adb -s SERIAL logcat -d -v threadtime > znet-debug-logcat.txt
```

日志可能包含其他应用和设备信息，分享前人工筛选、脱敏；不要自动上传。若设备
`logcat --help` 支持按 PID 过滤，可只收集目标进程。不要为抓日志自动清空已有日志。

## Chrome WebView 检查与兼容下限

电脑 Chrome 打开 `chrome://inspect/#devices`，启用 Discover USB devices，保持 Debug
应用前台，选择对应 WebView 的 Inspect。[Chrome 官方步骤](https://developer.chrome.com/docs/devtools/remote-debugging/webviews/)。
Inspector 可访问页面和宿主调用，调试请使用不含真实代理秘密的材料。Release 不应暴露
可检查 WebView；若发现，停止该 Release 的设备验收并报告。

本仓库 Vite 目标与 Tailwind4 的支持边界要求 Chromium WebView **111 或以上**；这不是
所有 Tauri 应用的通用下限。[Tailwind 浏览器要求](https://tailwindcss.com/docs/compatibility#browser-support)。
宿主先检查实际 UA，旧版本或不能识别的版本显示原生兼容提示，避免加载不兼容页面。
Android7/API24 是安装下限，不能保证 Galaxy S8 原厂 WebView 能渲染当前前端。
Android7–9 的实际 provider 可能由 Chrome 提供，不能仅按独立 WebView 应用版本判断。
[Chromium provider 说明](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/android_webview/docs/legacy-os-behavior.md)。
Chromium119 源码的标准 Android 下限为24，120为26；这仅说明历史构建支持，
不承诺设备当前可从商店取得特定版本或旧版本仍安全受支持。
[119 构建定义](https://raw.githubusercontent.com/chromium/chromium/119.0.6045.163/build/config/android/config.gni)、
[120 构建定义](https://raw.githubusercontent.com/chromium/chromium/120.0.6099.43/build/config/android/config.gni)。
设备上记录实际 provider、UA、API和可获得更新；不建议安装未知来源 WebView。

## 可以验证的范围与剩余条件

设备可检查四底栏、320px与系统字号、深浅色、安全区、键盘及返回/未保存提示；
普通配置编辑与离线保存、路由/DNS本地草稿、文件选择/保存/分享及取消、脱敏导出。
本地保存应显示待应用；不要把它记录成运行态已生效。

VPN授权与真实转发、随包内核装配、运行态控制/采样、测速、端点真实数据、平台DNS、
插件执行和移动安装器仍受 [依赖清单](../../todo.md) 约束，不在此 APK 中冒充成功。
许可目录仍有四项 Android 原始材料缺口，构建收据应如实保留。没有 iPhone VPN验收、
正式签名或发布。每次设备检查另记 APK SHA、证书、设备/provider版本、步骤和实际结果。

当前 develop 已到 `408327df53554c28d5efeda3ece65535cabf783c`，当前分支与它有真实冲突。
本批未合并或重置工作树；工作分支 push 运行完整现有检查加 Debug 检查。其成功只证明
精确 head，不证明与最新 develop 合并后的结果，也不表示 PR 已可合并。
