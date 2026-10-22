# iOS Debug 隔离 Keychain 诊断

现有 `unavailable / accepted:false` 只证明 SDK 安全拒绝，不能说明 protected-data gate、
Keychain 查询、创建或读取中的哪一步失败，也不能据此推断需要开发者账号或签名身份。
本链继续只在云端测试，不购买账号、创建 Apple 签名凭据、调整 entitlement 或修改 core。

## 两条独立结果

生产 `secureVaultApi` 仍先检查 `isProtectedDataAvailable`，再执行原来的只读元数据 probe。
Debug 只增加最近一次实际 gate、probe 是否执行、OSStatus 和原准入布尔值的内存记录。
原 query、拒绝条件、主密钥归属及 SDK 的拒绝路径保持；没有用诊断成功重新授予生产权限。

`inspectPluginVaultSmoke` 仅存在于 Debug。请求必须匹配显式启动的
`ZNET_IOS_UI_SMOKE_NONCE`，不能自填 account、service、查询或原生操作；同一原生串行队列
和 busy 准入继续适用。诊断使用独立 `diagnostic-` account，与 `master` 和正常 SDK
round-trip smoke 分开；只生成随机的临时测试材料，不访问用户或插件数据。

隔离探测即使生产 gate 拦截，也会对自己的临时条目执行原生查询，让系统返回实际状态。
它不解除系统保护，不增加认证弹窗，也不使用后备文件。初始读取不是未找到时不覆盖；
已有同 nonce 的诊断材料只清理并标明原阶段，不伪造新建成功。

## 有限公开事实

回执分别记录最近生产准入、当前 protected-data 状态、availability probe、创建前读取、
随机材料、创建、回读和清理。每步明确是否执行，未执行时 OSStatus 为缺失/null；
状态是原生 Int32，不拼接原生异常描述。回读只公开匹配布尔值；没有密钥、摘要、account、
service、URL、令牌或材料字段。临时数据可变副本尽力清理，不保证运行时所有副本擦除。

诊断 `roundTrip:true` 要求实际创建、匹配回读和清理成功；它只说明隔离原语路径通过。
SDK 的 `nativeSecureStorageAccepted` 仍要求原生产准入与实际宿主 AEAD/丢失密钥检查通过，
不能由诊断或模拟器的安全拒绝替代。生产仍不可用时三类 SDK 角色继续拒绝。

Rust DTO 拒绝额外字段、不一致的阶段、未执行却有 OSStatus 和伪造成功。JS 再次按固定
字段投影，Python 检查公开回执；后续 UI/许可失败仍可保留已验证的诊断事实。
桥接缺回执不自动重复探测。明确失败的阶段及清理错误分别保留，不把失败叫作回滚成功。

## 实测边界

本地 Rust、Node/Python 和 Swift Foundation 模型测试分别记录；本地没有 UIKit/Security
SDK 或真实 Keychain 运行环境。实际 gate/OSStatus/创建/回读必须来自本提交的云端 iOS
Simulator smoke。其结果不替代真实 iPhone、锁屏/重启、卸载重装或生产发行环境验收。
原生文件票据、浏览器返回、系统 VPN/Packet Tunnel/内核装配继续各自的范围与缺口。

`6fd3dc4` 的云端 PR Simulator job `118410447966` 实际读到 protected-data=true，
生产 probe、隔离 availability probe 及创建前读取均返回 **-34018**。停止在
`readBeforeCreate`，随机材料、创建、回读、清理均未执行，roundTrip=false；SDK 仍为
unavailable/accepted:false。这是实际拒绝事实，不是成功存取证据。

[Apple DTS 的诊断说明](https://developer.apple.com/forums/thread/114456)将该错误解释为
`errSecMissingEntitlement`，要求检查构建后的 App，而非项目里的 entitlement 输入。
因此完整 App 验证器增加只读 `codesign -d --entitlements -`，回执只公开固定状态、
App ID/Team ID 是否存在、bundle 后缀是否匹配、Keychain/App Group 数量与首项是否匹配
App ID。未知字段、标识符原值、原始 plist 和工具 stderr 不进入此摘要。读取失败、
空输出、畸形数据与已解析的空字典分别记录；它们不授予权限或改变 smoke 验收判断。
不重签 App，不创建凭据或填入模拟 Team ID，也不据此要求购买开发者会员。
该摘要需由后续实际 SDK 构建产生，本地 fixture 不证明真实 App 的 entitlement 内容。
