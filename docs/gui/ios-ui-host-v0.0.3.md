# iOS 主应用宿主与界面调试

本批复用现有 Svelte 四底栏、Rust mobile entry、`mobile_host/application.rs` 和 NativeIo Swift 插件，补标准 Tauri 主应用工程的可复现生成、完整 SDK 构建和 Simulator 烟测。最低 **iOS 16.4**；Swift 文档库自己的 15.0 下限不代表主应用下限。版本显示保留完整客户端 dev semver，Apple 数字 bundle version 单独映射。

VPN、Packet Tunnel extension、App Group、完整内核接入与原生 VPN 生命周期继续后置，见 [todo](../../todo.md)。主应用能编译、启动或展示页面不等于这些能力完成。`gui_mobile_platform_snapshot` 仍返回 VPN unsupported、内核未随包可运行。iOS 原生许可 reader 现读取固定包内资产；完整宿主构建增加实际 Rust/Vite/各 SwiftPM 输入收集、标准资源再构建与最终资产校验，见[许可清单](dependency-license-inventory-v0.0.3.md)。普通开发构建未生成资产时明确 missing；材料与逐包最终链接证明的缺口仍保留，当前提交的执行结果以 CI 为准。

## 工程所有权与生成

`pnpm-lock.yaml` 锁定 `@tauri-apps/cli` **2.11.4**；Linux 发行的 CLI 不含 `ios` 子命令。必须在 macOS/Xcode 上用 `pnpm ios:init`，它调用标准 `pnpm tauri ios init --ci --skip-targets-install`。该命令生成 `src-tauri/gen/apple` 下的 Sources、Assets、project.yml、Xcode 工程，并由锁定 Tauri 构建逻辑链接插件。生成目录和机器绝对路径不提交；自有配置、标准 CLI wrapper、验证器和烟测驱动才是维护输入。不另建 SwiftUI 页面替代现有 UI，不手工拼装 Rust/Swift 链接关系。

生成器保留已有生成文件；修改过本机工程后不要靠反复 init 当作迁移。升级 CLI 时在独立干净目录重新生成、比较工程和插件链接，再迁移自己的改动，保留原工程。

依据已核对的 [锁定 init 入口](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-cli/src/mobile/ios/mod.rs)、[标准工程生成](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-cli/src/mobile/ios/project.rs) 及 [no-sign 构建路径](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-cli/src/mobile/ios/build.rs)。Xcode 的 Rust build phase 需要运行中的 Tauri CLI options 服务；直接打开工程后脱离 CLI 构建可能报 missing addr file，不能用假 options 或另一份 Rust 入口绕过。

## 云端验证

`.github/workflows/ios-app-host.yml` 在 macOS runner 分别执行：

| 范围 | 实际命令与证据 | 不代表 |
| --- | --- | --- |
| Simulator 完整 App | `pnpm ios:build:simulator`；标准 init、完整 Rust/Swift/资源链接、无签名身份的 Debug App、实际 Mach-O SDK/架构/入口/插件符号和内嵌前端核对 | 真实 iPhone、发行签名、VPN |
| Simulator 启动和烟测 | 安装并启动上一步真实 `.app`，真实 WKWebView 完成 onboarding、四底栏、关于/返回、真实 Rust IPC 与诚实 unsupported/许可状态；nonce 防旧回执；保留 screenshot、launch、JSON | 完整 PRD 或真机生命周期、文件提供者验收 |
| arm64 设备 SDK | `pnpm ios:build:device`；完整 unsigned iPhoneOS App，单独 SDK/arm64/包内入口核对 | 可安装到未配对 iPhone、provisioning、真机通过 |

构建脚本拒绝签名/Team/API 凭据环境，使用 CLI `--no-sign`，不导入/创建证书、私钥、provisioning 或 Apple 账号，不提交商店、不发布。Simulator/App 构建日志与产物按当前 commit 独立保留；`package-receipt.json` 记录的构建结果与 `simulator-smoke.json` 的执行结果分开。PR workflow 可能检出 GitHub 合成 merge ref；分支 push 的产物回执直接绑定 head。以实际对应 run 的终态和回执为准，脚本存在不计构建已成功。

Apple arm64 Simulator 工具链可在 `--no-sign` 下输出 ad-hoc Mach-O 签名，见 [Apple linker 默认逻辑](https://github.com/apple-oss-distributions/ld64/blob/main/src/ld/Options.cpp)。验证器只在 Simulator 接受 `Signature=adhoc`、无 Team、无 Authority 的结果，单独记录 `signature=ad-hoc`、`unsigned=false`，不称为 unsigned 或设备发行签名；设备 SDK 仍要求真正 unsigned。原始 `codesign-details.txt` 随证据保存，任何证书身份或 provisioning 都拒绝。

烟测仅在 NativeIo 的 **Swift Debug 条件 + 显式 32 位随机 launch nonce + 固定随包测试驱动存在**时运行。普通本机调试不设置该环境变量就不会执行驱动；Release 无执行入口。驱动只通过现有 UI/IPC 操作本次独立 Simulator 容器，不注入代理数据、许可证空清单、采样或假成功，不创建 VPN、不打开系统文件/通知权限。回执仅写固定私有 Caches 路径。CI 首轮前保留 `executionVerified=false`，实际烟测另外产生正向证据。

CI 按实际发现的可用 iPhone 设备选择最高数字版本 runtime，记录实际 OS 与 SDK，收集有界原生 console 并对 boot/launch/回执等待设限。包检查还要求实际 Mach-O 包含 Debug 探针。iOS 18.5 Simulator 存在 [WebKit Swift overlay 已知启动缺陷](https://bugs.webkit.org/show_bug.cgi?id=293831)；历史 18.5 未取得回执不算通过，新 runtime 的通过也不代表 16.4 或 18.5 已验收。应用最低版本仍为 16.4，不通过提高最低版本掩盖未运行项。

## 用户本机最小步骤

这些步骤由用户在自己的 Mac 执行；本批云端实现没有操作用户 Mac。

1. 安装并首次打开 Xcode，准备 iOS Simulator runtime、Rust、Node >=22.12、pnpm 9、XcodeGen、CocoaPods。用 `xcode-select -p` / `xcodebuild -version` 确认完整 Xcode，不只是 Command Line Tools。安装 Rust iOS 目标：

   ```sh
   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
   pnpm install --frozen-lockfile
   pnpm ios:init
   ```

2. 优先 Simulator 界面调试：

   ```sh
   pnpm ios:dev --open
   ```

   保留该终端与 Vite/CLI 进程，Xcode 选择生成的主 App scheme 和已安装的 iPhone Simulator，再 Run。四底栏必须来自现有客户端；首次完成 onboarding，然后检查概览、节点、配置、更多→关于及返回。没有配置/内核时应有解释，不显示 VPN 已接管。

3. 真实 iPhone：用户自行用 USB 配对、信任此电脑、开启 Developer Mode，并在本机 Xcode 的 Signing & Capabilities 选择自己的 Team、处理签名和安装结果。云端不提供账号、证书或 profile，不要求购买账号。Personal Team 是否满足当前本机安装条件以 Xcode 回执为准，UI 安装资格不等于 Network Extension/VPN entitlement。需要设备连接本机 Vite 时：

   ```sh
   pnpm ios:dev --host <你的Mac局域网IP> --open
   ```

   iPhone 与 Mac 网络可达，Vite 1420 与 HMR 1421 可访问；`TAURI_DEV_HOST` 由锁定 CLI 传给已有 Vite 配置。Debug overlay 仅为这条显式开发路径开放本地网络/网页 HTTP；标准随包构建 `devUrl=null`，不用云端 CI 的开发服务器地址。先检查开发 URL 和网络授权，不能把白屏误报为 VPN 问题。

4. Safari Inspector：Mac Safari 开启开发者菜单，iPhone Safari 高级设置开启 Web Inspector；选择 Develop → 对应 Simulator/设备 → ZNet Sink WebView。Debug inspector 可用，Release 不主动启用 devtools Cargo feature。分别在 Safari 看 WebView console/network、在保留的 CLI/Xcode console 看 Rust/native 错误。官方说明见 [Tauri Debug](https://v2.tauri.app/develop/debug/)、[WebKit Inspector](https://webkit.org/web-inspector/enabling-web-inspector/) 和 [Apple Developer Mode](https://developer.apple.com/documentation/xcode/enabling-developer-mode-on-a-device)。

5. 用户最小验收：记录 iPhone 型号、iOS、Xcode、commit；启动与切后台/前台；核对四底栏、安全区、系统字号、表单软键盘、关于版本与实际许可状态（无资产的开发构建为 missing）；取消一次系统文件选择应保留原草稿。未进行的 VPN/锁屏/撤权/网络切换和真实文件提供者场景逐项保留未验收。

CI 的 Simulator `.app` 只用于 Simulator，unsigned iPhoneOS 产品也不能当作已签名可安装包。完整真机验收结果需用户本机反馈；不从截图或库编译推断。
