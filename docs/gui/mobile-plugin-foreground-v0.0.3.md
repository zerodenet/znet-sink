# 移动插件前台宿主与安装审阅

从已验证 `2b411b6` 继续，同一工作分支和 Draft PR #46。此处描述客户端插件工作，
不注册 VPN 服务、Packet Tunnel 或内核 owner，也不宣称完成 A20–A22 真机验收。

## 原生所有权与准入

`services/plugins/platform.rs` 使用原生设备事实创建 Target。Android 从系统配置读取
phone/tablet，iOS 从 UIDevice idiom 读取；OS、ABI 来自实际 Rust 编译目标。
没有设备事实时保持 starting/unavailable，不默认填 phone。目标须满足签名组件声明、
宿主版本、权限、能力和 VM 隔离要求。要求 process isolation 的组件继续拒绝。

QuickJS 是同进程非 JIT 解释器，采用现有有界 executor、独立队列期限、原授权 lease、
CPU/墙钟/内存/输出预算。不能隔离 native crash、OOM 或操作系统终止。Android/iOS
由真实 SDK 生成 QuickJS C bindings，不套用 Linux ABI；Android 构建须使 NDK bin
位于 PATH 首位且提供 libclang。最低 Android 仍是 API24，不代表 Galaxy S8 已验收。

原生后台事件立即封闭准入、关闭原 executor、取消文档并释放待审阅包；不等待安装/目录
网络锁，不声称撤回已经发出的原生操作。前台先重读设备、核对代次，才建立新 executor
和空文档通道。旧文档不复活。执行时在 owning operation 锁内冻结 executor 与 review；
不能排队到新 owner 后借新授权执行。HostStart 仅在当前 Host 一次性提交原审阅，并携带
真实 AppHandle 进入共享 SDK；恢复不重复启动旧 hook。此处不承诺 OS 后台定时任务。

当前 iOS Release 尚未验收外部代码分发，禁止外部插件安装/运行；Debug 只开放显式
审阅的开发插件。该限制是尚未适配发行形态，不宣称平台普遍禁止插件。签名发行、商店
审核和真实设备运行仍未完成，不创建账号、签名身份或购买凭据。

## 系统选择与不可重放安装

移动本地安装使用 native-io 的 `plugin-package-import`，仅只读系统选择，取消不提交。
压缩包读取上限与签名分发层一致，为 16 MiB；验签、解压和结构预算沿用原分发层。
页面得不到 OS 路径或包正文，不使用桌面 dialog、Blob 下载或 guest 自报 selection。
系统选择临时后台时，等待可信原生恢复；超时明确失败，不伪造 foreground。

宿主最多保存两个待审阅包，单调时钟有效期五分钟；随机 opaque plan 固定所选字节、
候选摘要、原发布者指纹、当前版本和宿主代次。首次本地信任仍独立确认。确认即消费 plan，
在 owning operation 锁内重读材料及代次，然后使用现有签名校验和原子安装链。
取消、错误摘要、过期、被替换和重复确认均拒绝。丢失回执不能自动再执行，先核对已安装
状态，必要时由用户重新选择和审阅。安装成功不代表授权或启用。

线上安装携带原审阅代次，并在下载后再次核对；移动安装必须固定候选摘要。即使没有新增
权限，下载包变化也不能越过原审阅。安装、启用、配置和授权仍分别建模。

## 页面与 SDK

配置草稿保存打开时的 review，不因刷新换成新材料身份；拒绝时保留草稿。用户可明确
“放弃草稿并重新载入”。停用/撤权同样检查原 review。原生代次变化使旧确认失效。
未核对的安装读取不显示权威空列表；目录失败、真正为空和设备身份未知分别表达。

普通网络、隔离状态/缓存、现有托管来源与日志契约保留实际 permission/scope/budget
检查。通知先核对应用意图和系统授权，不为普通读取请求权限；系统提交不保证通知呈现。
VM/原生 SDK 是否适配必须逐能力判断。[安全存储适配](mobile-plugin-secure-storage-v0.0.3.md)
已接入原生 API 与原组件 lease；真实运行验收单独记录，不回退明文主密钥。文件选择票据、
浏览器回调、系统调度及受保护运行材料加载仍展示各自未适配原因并拒绝调用，不把它们
整体归为 core 阻塞或已获后置。后续继续实现独立客户端部分。

## 验证边界

实际结果见[统一验证记录](../verification/mobile-client-v0.0.3.md)。Linux 固定 VM 检查
不替代手机运行。iOS Debug 的显式每次启动 nonce 仅启用固定源的 VM 验证：真实解释器、
身份、未授予拒绝、超时、撤权；不接受 guest 源码，不读用户插件、文件或秘密。
Simulator 流程还启动系统 Settings，再将同一应用进程带回前台，核对原生代次与 executor
封闭/重开。验证器必须拒绝进程重建、旧 nonce、旧代次和后台仍准入；脚本存在不算通过。
设备前后台、锁屏/低内存、Galaxy S8、iPhone、VPN 转发、签名和安装仍单独记录。
