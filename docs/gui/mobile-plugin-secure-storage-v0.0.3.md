# 移动插件安全存储适配

本批继续同一 Draft PR #46，属于客户端 SDK，不是 VPN 或 core 运行接入。
长期秘密和设备密码学沿用现有 SDK、发布者/插件命名空间、组件授权和 AEAD；不新增业务密钥协议。

## 主密钥归属

Android API24 使用 [AndroidKeyStore](https://developer.android.com/privacy-and-security/keystore) 中不可导出的 AES-256 密钥包装随机 32 字节主密钥。
仅在应用私有 noBackup 容器保存固定 61 字节 AES-GCM 信封；AAD 绑定实际应用标识，
创建采用跨进程文件锁和 AtomicFile，返回前重新读取并解密。JVM AES 测试不证明
AndroidKeyStore；硬件、StrongBox、Galaxy S8 或备份恢复未据此验收。

iOS 使用 Keychain generic-password，[AfterFirstUnlockThisDeviceOnly](https://developer.apple.com/documentation/security/ksecattraccessibleafterfirstunlockthisdeviceonly)、不同步、无认证弹窗。
只使用当前应用身份的固定 service/account；不配置 App Group、不创建签名凭据，不声称
Secure Enclave。原生资料保护及 Keychain 元数据查询只用于能力准入，真正读写仍可失败。
失败保持不可用，不要求用户重新授权插件来掩盖平台密钥丢失。

主密钥经 Rust-only native-io 私有方法进入宿主，不注册 WebView 调用、事件或 guest DTO。
Rust 接收/解码使用 Zeroizing，原生实现尽力清理可变副本；不保证 Swift/JVM/序列化运行时
所有不可变副本可被擦除。插件只取得其权限允许的秘密值或密码学结果，不能取得主密钥。

## 丢失、取消和并发

原生回执严格检查状态、字段和 32 字节长度。移动端不调用桌面 vault.key 后端；发现旧明文
主密钥格式拒绝降级。宿主持久保存有界、不可跟随符号链接的公开密钥身份标记；该标记不含
主密钥。有标记时只允许读取现有平台密钥，丢失或身份改变均拒绝创建替代。Android已有信封
但包装密钥丢失或密文损坏同样拒绝替换。恢复与迁移需要独立、明确的材料流程。

SDK 调用在原组件 lease、安装身份和宿主代次下执行；原生 IO 前后及持久发布前再次核对。
停用、撤权、后台或换代后的结果不交付；不声称撤回已经发出的原生操作。原生安全存储采用
有界串行工作队列和 busy 准入。已核对的安装快照可读取内存缓存，不被安全存储 IO 锁阻塞。
UI 的重新检查不能伪造原生前台事件或让后台宿主重新开放执行。

secureStorageAdapter 表示原生 API 已核对，不代表每次操作或真机已经通过。实际失败向 SDK
返回固定安全的 transport 原因，不传播系统异常、路径、URL 或密钥，不自动重试写入。
文件票据、浏览器回调、OS 调度及受保护运行加载仍保持各自未适配原因，不随本能力开放。

## 独立验证

Android新增隔离测试 APK，在实际 API24 模拟器中测试原生包装、恢复、不可导出、损坏及
丢失密钥拒绝替换。仅在明确的 Debug 测试包和空测试容器运行，清理仅限本次创建的材料；
复用未知容器直接失败。SDK Debug 测试签名仅存在临时测试 home，不是发行身份。
普通 native-io 检查构建该 APK；connected 检查必须显式开启并有真实模拟器，不能把 APK
构建或 JVM 测试当作运行成功。

iOS Debug 显式启动 nonce 限定独立 smoke Keychain account，不读写正常 master 或用户
插件资料。真实原生创建/读取后验证宿主 AEAD、删除本次测试密钥、确认旧身份拒绝重建；
失败仍清理本次 account，只回传布尔值。Release 拒绝 smoke selector。API 不可用时三类
SDK 角色必须全部阻止，收据标记 unavailable/accepted:false；该分支只证明安全拒绝，
不计为安全存储可用。Apple SDK、Simulator 与真机结果分别记入统一验证账本。

准确结果见[验证记录](../verification/mobile-client-v0.0.3.md)。真实 iPhone、Galaxy S8、
锁屏、重启、卸载/重装、系统备份恢复、低内存及发行签名环境仍需独立验收。
