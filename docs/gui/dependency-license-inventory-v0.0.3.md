# 本次构建的依赖许可目录

本目录记录可追溯的构建材料，不给出法律合规结论。依赖图、编译输出、最终产物包含是不同证据；有声明而没有正文、缺少具体归属资料、无法确认实际 Swift pin 等情况必须保留缺口。生成文件不手工复制为长期版本清单，版本、正文、输入摘要随本次构建变化。

## 输入与证据

| 来源 | 生成与记录 | 不能推断的结论 |
| --- | --- | --- |
| Vite client build | `scripts/license-vite.mjs` 观察实际 transform、Tailwind 注册的 CSS 输入、chunk 的非零 rendered JavaScript，以及与真实 package 文件逐字节匹配的输出字体/SVG，按 installed manifest 与 pnpm exact integrity 归属 | 安装了某个 npm 包不表示已打包；CSS 输入不证明样式全部保留；配置导入的工具单独记 build-tool |
| Rust target | `scripts/license-rust.mjs` 从 `cargo metadata --locked --filter-platform` 和明确 feature 集合遍历 normal/build/proc-macro，排除 dev；收集 crate 真实文件和有明确 workspace-inheritance 的 legal material | Cargo.lock 全集合不是手机运行集合；resolved-target 不是 compiled；可选 compiler-artifact 仅确认已有 code-generated 输出，不证明死代码链接保留 |
| Rust standard library | 当前 rustc/sysroot 对应目标库存在，记录 toolchain 身份、随工具链提供的 library copyright 与许可条款 | source-level legal inventory 内的每个组件不一定进入最后二进制，不创造一个覆盖所有库的 SPDX 表达式 |
| Android | `scripts/license-gradle.init.gradle` 收集选中 release runtimeClasspath 的实际 AAR/JAR/source legal files、artifact/POM SHA、固定 GAV 与有界 parent POM 许可继承 | runtimeClasspath 已解析不等于 R8 保留每个类；声明 URL 不表示包自身提供了全文 |
| iOS | `scripts/license-swift.mjs` 从原始 locked Cargo Tauri source、真实 NativeIO Package.resolved、匹配 checkout revision、实际 SDK metadata 与无签名编译 log 收集 | Cargo swift-rs 版本不替代 SwiftPM pin；SDK framework 是 system-provided，SDK 元数据不等于再分发许可；Linux Foundation 测试不证明 UIKit 编译 |
| 最终 Android APK | `scripts/license-artifact.mjs` 检查实际 `assets/license-inventory.native.json`、对应 frontend JSON SHA、Android release target、arm64 `libgui_lib.so`，产生外部 final APK/asset/native payload SHA receipt | 产物包含清单和 native payload 不证明每个 Cargo crate / Maven class 最终保留，也不能把外部 receipt 假称原 APK 内已嵌入的收据 |

材料扫描使用 UTF-8 严格解码、单文件预算，排除依赖目录/构建缓存；`.spdx` 是元数据/notice，不当作完整许可。原文以内容摘要去重，但每个组件保留其材料引用和来源。UI 仅展示纯文本；未知声明不默认补 MIT/Apache/MPL。

## 离线可复用的共同条款

`resources/licenses/authoritative-materials.json` 记录本次固定 Maven `jackson-annotations:2.15.3` archive 提供的完整 Apache 2.0 共同条款、原始 archive evidence 和正文 SHA。只有当前 POM 明确引用相同官方 canonical URL 才能复用；不会把 Jackson 的版权/NOTICE 转给另一组件。官方 `https://www.apache.org/licenses/LICENSE-2.0.txt` 在本次云端 shell 下载返回 HTTP 403，缓存的 acquisition 字段明确说明实际来源，不能称作成功抓取官方正文。

复用后正文可读，但原包未提供自己的材料仍保留 `package-specific-legal-material-not-provided`。其他 SPDX 声明不靠模板补版权。网络获取只在显式维护 `license-materials.mjs --write-cache` 时进行，正常构建只用已审阅缓存。剩余 missing 是交付资料缺口，不因列表已生成就变为“全部许可完成”。

## 精确版本的原始包材料

`resources/licenses/{frontend,rust,android,swift}-package-materials.json` 分别保存锁定包的真实 LICENSE、NOTICE、COPYRIGHT 或逐字节截取的源文件版权注释。每条材料保留来源 URL、实际下载体/归档 SHA、成员路径、Git revision（存在时）和正文 SHA；截取注释同时记录原字节范围。源文件短注释仅算 notice，不能当完整许可。缓存中的 acquisition/audit 记录实际检索和覆盖范围，未取得或未核实的内容继续保留缺口。

共享 `applyPackageMaterialCache` 要求 ecosystem/name/version/source 四字段完全一致，并核对每条 expected 输入：npm manifest SHA 与 pnpm integrity、Cargo manifest/package ID/实际 archive SHA、Maven selected artifact 与 POM SHA、Swift 实际 pin/checkout 和声明文件 SHA。已有身份的输入漂移直接报错，不以新版套旧版；SPDX 不用于推断版权。只有原始许可正文补正文缺口，具体包的原始归属材料只处理本包缺口，不补许可声明、Swift pin、平台支持或最终链接证明。缓存内容仍需审阅，哈希用于身份与新鲜度校验，不等同于签名或法律结论。

正常构建离线消费这些材料。新增下载只由显式维护收集器进行，例如 `license-android-packages.py --inventory ... --download-cache ... --output ...`；不会在 Gradle/Cargo/Vite build 内访问许可网站。清单记录缓存原文件 SHA；Rust release hook、Android merge 与最终 APK receipt 拒绝缺失或过期的材料输入。私有客户端 crate 的 MPL 声明显式引用仓库现有 LICENSE，不借第三方材料补自有包。

当前材料仍有明确限制：`svelte-toolbelt@0.10.6` 有原始正文但没有包级许可声明；`rustls-platform-verifier-android@0.1.1` 发布包无原始全文或可核实的 VCS 版本；`selectors@0.36.1` 的原包 MPL 注释与明确指向的共同条款可读，包专属版权归属仍未提供；`tao-macros@0.1.3` 的实际 tag 与发布源码逐文件匹配，仅取到 Apache 原文，另一可选 MIT 全文仍缺失。Swift 缓存有固定 revision 原文，但没有实际构建 pin 时仍 unresolved。共同 MPL 条款来源如实标为客户端固定提交中未增加组件归属的标准全文，官方 Mozilla 获取 403 不写成成功，绝不把客户端作者或 sibling crate 版权转给 selectors。

### 四项缺口的独立复核

本轮不改变这四项缺口数量，也不修改已有 APK。补充的材料进入下一次真实构建，其身份与 cache SHA 仍由现有生成、打包检查验证。

- `svelte-toolbelt@0.10.6`：从官方 npm registry 取得 exact tarball，SHA-512 与 pnpm lock 和官方版本元数据一致；原始 `package/package.json` 与 `package/LICENSE` 又与安装目录逐字节相同。该 LICENSE 明写 MIT License，并保留 Hunter Johnston、Thomas G. Lopes 原始版权。新 `reviewedLicenseMapping` 仅将这个已审阅正文映射为 MIT，绑定确切版本、manifest SHA、lock integrity、archive SHA 与正文 SHA；`declaredLicense` 仍为 null，`license-declaration-not-provided` 继续存在。About 单独显示“已审阅正文映射；包 manifest 未声明许可”，不把维护者复核说成发行包自带 SPDX 声明。
- `selectors@0.36.1`：按发布包 VCS `635e1a19d02960588a00e189bd4bd5bdb150ec3d` 核对 Servo/stylo root 与 selectors 目录，并复核完整发布源文件集合。该范围只有既有 MPL 头；manifest 的 `The Servo Project Developers` 是作者元数据，不能补造带年份的版权声明。未观察到本包额外版权/NOTICE，缺口保留；不借 style、servo_arc 或 Mozilla 其他组件的归属材料。
- `tao-macros@0.1.3`：精确 tag commit `340684d932b6126bed449fc1f4edca81776ad592` 已与发布源逐文件匹配；本轮再核 root `LICENSE.spdx` 明确属于 `PackageName: tao`、声明 Apache-2.0。不能将其 Tao 版权冒作宏包 MIT 材料，缺失的 MIT 全文仍保留。
- `rustls-platform-verifier-android@0.1.1`：官方 crates.io 版本 metadata checksum 与 lock/archive 相同；完整发布包及其 AAR、嵌套 classes.jar 均未观察到 LICENSE/NOTICE/COPYRIGHT 或不可变 VCS 记录。官方 GitHub repository API 获取返回 proxy tunnel HTTP 403，未换域绕过。官方元数据中的 MIT OR Apache-2.0 声明不替代缺失原文，不借同仓其他版本材料。

以上检索范围、原始 archive 成员、AAR SHA 和阻塞结果保留在各 package/gap 的 `followUpAudit`。它们描述实际观察范围，不声称已经穷尽世界上所有法律资料。

`scripts/license-review-svelte-toolbelt.mjs` 是显式离线维护器，输入维护者已下载的官方 tarball、原 registry metadata、安装目录与 pnpm lock，拒绝输入漂移后才更新 frontend cache。正常构建没有新增网络访问。`scripts/license-reviewed-mapping.mjs` 只接受此包已审阅的完整身份和正文摘要，并要求当前安装包实际提供相同 LICENSE；不能用缓存遮盖安装文件变化。测试在既有 `test-license-package-materials.mjs` 与 `test-mobile-about.mjs` 中运行实际生产函数，覆盖错版本、错包、锁/manifest/原文漂移、错误映射与伪称 manifest 声明。

## 构建接线

1. `pnpm build` 自动通过 Vite 插件产生 `build/license-inventory.frontend.json`（client only）。`build/` 已忽略。
2. 显式预收集 Rust（默认移动宿主保留三个 tool features）：

   ```sh
   node scripts/license-rust.mjs --target aarch64-linux-android --profile release \
     --features tool-dns,tool-route,tool-node-probe --output "$RUNNER_TEMP/rust-license.json"
   ```

   `ZNET_LICENSE_INVENTORY` 指向此绝对路径；实际发行 job 设置 `ZNET_REQUIRE_LICENSE_INVENTORY=1`。`src-tauri/build_license_inventory.rs` 只读取与检查 target/profile/features、当前 Cargo.toml/Cargo.lock 与 Rust 材料缓存 SHA、正文 SHA/引用预算，然后复制到 OUT_DIR；不运行 Node、不嵌套 cargo、不开网络。strict 仅作用 release；CLI debug 初始化或普通 Cargo 开发编译可以产生带原因的 unavailable stub。
3. 真实 Tauri Android CLI 的 extra args 是 Cargo args，不用来传 Gradle `-I`。在 job 专用 `GRADLE_USER_HOME` 设置以下绝对路径环境变量，再运行 `node scripts/install-license-android-hook.mjs`：

   ```text
   ORG_GRADLE_PROJECT_znetLicenseMergeScript=scripts/license-native-merge.mjs 的绝对路径
   ORG_GRADLE_PROJECT_znetLicenseCargoInventory=Rust JSON 的绝对路径
   ORG_GRADLE_PROJECT_znetLicenseFrontendInventory=frontend JSON 的绝对路径
   ORG_GRADLE_PROJECT_znetLicenseMaterialsCache=已审阅 cache 的绝对路径
   ```

   安装器只复制 reviewable init script 到该 job 的 init.d。AGP selected release variant 的公开 generated-assets API 将 aggregate task outputDirectory 绑定到 mergeAssets，生成固定 `license-inventory.native.json`；遵循 AGP 实际设置的目录，不假定输出目录名。普通桌面产物不会带 Android target 清单。临时 Gradle/Cargo 目录已在各自 ignore 规则内。
4. `pnpm tauri android build --apk --target aarch64 --ci` 进行真实 unsigned host build；不创建 debug/发行密钥。产物后运行：

   ```sh
   node scripts/license-artifact.mjs --apk ACTUAL_APK \
     --frontend build/license-inventory.frontend.json \
     --output external-licensed-build-inventory.json --receipt external-package-receipt.json
   ```

   缺固定 asset、错误 target、过期 frontend SHA 或缺 native payload 直接失败。外部 receipt 不追溯修改 APK。若没有实际 compiler JSON，则 Rust entries 继续标为 resolved，不能把普通 build log 当各包编译证明。
5. iOS 收集需传入实际 `--cargo-metadata`、`--swift-build-path`（SourcePackages）、`--build-log`、`--target`，可传 `--resolved-file`、`--sdk-path/--sdk-data`、`--artifact`。目标从实际 Swift compiler `-target`/Mach-O 而不是 runner 名推断；pin/checkout/SDK/architecture 不符不能声明编译成功。当前尚无 iOS application host，Swift 清单作为 unsigned SDK 构建证据，主应用原生 reader 明确 unavailable；不声称该清单已进入 iPhone 产品。

## 读取与验证

About 消费前端 fixed JSON、`app_license_inventory` 中的 Rust embedded_snapshot 及原生固定 asset。宿主主窗口准入；Android `readLicenseInventory` 不接收任意路径/URI，也不走 SAF grant。16 MiB 预算和 available/missing/invalid 分开；Desktop/iOS 未实现固定 app asset reader 时保留 unavailable。取消/文件权限与许可只读读取无关。

- `node scripts/test-license-inventory.mjs`：SPDX 非全文、UTF-8、依赖目录隔离、摘要篡改、引用缺失、确定性、来源脱敏、明确 POM URL 绑定及 Android target/features 冲突。
- `pnpm test:licenses` 同时执行包身份/缓存新鲜度回归、实际 Vite/Tailwind 字体与 CSS 构建、Swift 原始 archive/Git blob/SDK 目标校验及 Android 原字节版权截取测试；fixture 通过不表示手机或 Apple 构建已验收。
- 完成一次 workspace compile 后 `node scripts/test-license-build-hook.mjs` 使用已构建 sha2/serde_json 编译实际 Rust hook fixture，检查 release target/features/input SHA/text 摘要拒绝以及 debug honest stub；不重建 GUI。
- `scripts/test-mobile-about.mjs` 校验清单解析与未知材料状态；`tests/ui-controls/mobile-about.spec.ts` 320 px Chromium 检查搜索/折叠/正文末尾、44 px 目标、保留缺口、无自动外部发送。
- NativeIO Rust receipt tests 拒绝畸形状态、原始路径/URL error 与超预算正文；Gradle 实际 merge 验证 fixed asset 进入 AGP merged assets；最终 APK receipt 是独立更强的产物检查。

本地生成数目随当前依赖和 target 变化，CI 应保留 JSON 和日志而不是断言固定数量。Android/iOS 真机 UI、商店、签名及真实 VPN 不属于这些构建/许可证据。
