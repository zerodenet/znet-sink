# 移动适配依赖与暂不支持项

2026-10-14，v0.0.3，Draft PR #46。这里是依赖协调与解除条件的单一入口；完整 PRD 范围和已实现项沿用 [验收账本](docs/gui/mobile-acceptance-ledger-v0.0.3.md)，不另造完成标准。源码检查不是手机验收，unsupported 占位不计为已完成 VPN。

已核实客户端 develop 为 `fd198ba52711f018334860b8a6704546ccd349fd`；客户端依赖、只读 core 审计及 core 远端 develop 为 `e74561f99acca941bfadf0d33e35a5ef08f34086`。只修改客户端；core 由用户协调其负责人。下列行为要求是建议验收契约，接口名称尚未决定。

## CORE-01

**必需阻塞：外部平台隧道接入。依赖方：core 的 tun/proxy/stack 及客户端原生宿主。**

证据：[`TunDevice::into_channels[_observed]`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/tun/src/lib.rs) 已存在；`adopt` 只在 Linux/macOS 导出。Proxy 的 [`start_tun_internal`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/proxy/src/inbound/tun.rs) 仍自行 create/configure，并走桌面路由安装。

影响：Android VpnService 的 FD、iOS Packet Tunnel 的 packetFlow 尚不能正式注入完整 Proxy，不能开启或确认移动 VPN。

解除条件：支持平台已经配置的设备或包通道进入既有 stack/proxy；明确包方向、IPv4/IPv6、MTU、背压、错误、关闭所有权与真实设备统计。不重新创建桌面接口或重复配置系统 DNS/路由。

验收：真机 TCP/UDP/DNS/Fake-IP 和声明协议可双向转发；停止和撤权释放资源；背压、接口失败和服务重建可核对，不假报接管。

## CORE-02

**必需阻塞：移动外层 socket 保护与物理网络绑定。依赖方：core traits/platform/各实际拨号路径及客户端宿主。**

证据：[`EgressInterfaceControl`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/platform/tokio/src/egress.rs) 已有 generation、subscribe_generation、invalidate_network、双栈出口和桌面绑定；Android/iOS 对应绑定分支返回 Unsupported。[socket 构造路径](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/platform/tokio/src/lib.rs) 没有移动宿主保护回调。不能说内核完全没有网络代次机制。

影响：Android 需要连接/发送前 protect 与 Network 绑定；iOS 需 provider 允许的外网连接路径。未接入时不能保证外层连接不回送 VPN，不开放接管。

解除条件：所有真实 TCP/UDP/DNS/QUIC/协议外层 socket 在第一次网络 IO 前进入正确保护/绑定边界，失败拒绝拨号；平台网络身份及路径变化接入现有代次/失效机制，旧连接池与拨号结果不能继续代表新网络。

验收：Wi-Fi/蜂窝切换、双栈/IPv6-only、断网及恢复无回环；保护失败明确失败；旧网络 late result 不宣称恢复成功。物理出口缺失与禁用 IPv6 分开表达。

## CORE-03

**特定写操作安全缺口；不是所有操作的统一阻塞。依赖方：core API 与 owning command。**

证据：[通用 ConfigApplyCommand/ModeSetCommand](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/api/src/command.rs) 没有相应 expected instance/config revision。端点、统计已有自己的前置条件；[`ProxyHandle::apply_runtime_config_if_current_with_principal_impact_and_wait`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/proxy/src/runtime/handle/model.rs) 已在配置应用锁内比较当前配置。

影响：Android/iOS 服务重建或另一个控制端在读取和提交之间改变配置时，客户端本地 CAS、固定连接及前后查询不能冒充通用远端原子保证。

解除条件：对确有需要的公共写命令提供兼容的身份/版本前置条件，并在 owning command 同一协调边界校验。Rust 内嵌可先复用现有条件应用路径；不向已具备条件的端点/统计重新发明一套契约。

验收：两个控制端竞争、同名资源重建及实例重启使旧写操作在副作用前被拒绝；超时先核对、不自动重放。无安全条件的能力保留明确限制。

## CORE-04

**条件增强：统一外语 FFI 的完整生命周期；不是 Rust 内嵌的必需前置条件。依赖方：选定桥接方案后由 core/客户端协调。**

证据：[当前 C FFI](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/ffi/src/lib.rs) 创建 EngineHandle；公开 Rust [Proxy::new/spawn](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/proxy/src/runtime.rs)、[RunningProxy::shutdown](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/proxy/src/runtime/running.rs) 和 ProxyHandle 的查询/命令/事件服务已存在。

影响与解除条件：客户端 M01/M04 可独立基于公开 Rust 接口实现；仅当采用 C 边界时才扩展该包装，范围限完整运行所有权、停止和已有控制/事件服务，不把客户端安装、订阅或插件业务塞进 core。

验收：原生服务/扩展持有实际 Proxy，UI 退出不终止转发；停止完成资源释放；不能用 EngineHandle 的控制状态冒充数据面就绪。

## CLIENT-01

**原生 runtime owner、默认随包内核与控制桥：客户端继续实施。** 本轮增加共享 Rust 窄控制桥、实际 IPC 只读检查和独立监听取消/缺口处理；native owner 与 EventSource 尚未注册，不计为完整 M04。 已有公开 Rust 生命周期/控制服务可复用。当前没有真实 VpnService/Packet Tunnel runtime 注册或可运行的默认随包内核，移动调用返回 unsupported，平台 VPN 状态保留 unknown。解除条件为实际宿主所有权、随包编译能力、鉴权/有界控制与事件桥及 CORE-01/02 组合验证；验收包括撤权、主动断开、实例换代和不依赖 UI 存活。

## CLIENT-02

**后台订阅和更新检查：客户端及平台调度。** 本轮实施真实前台到期协调，后台 OS 调度仍未适配，不注册空 Worker 或返回假成功。解除条件为系统允许的调度、持久去重和恢复核对；验收区分周期意图、实际执行、推迟、进程中断和结果未确认，不承诺准点。

## CLIENT-03

**活动连接导出、捕获恢复记录：本轮源码已实现。** 实际 pinned IPC 固定实例/筛选，交付前再核对，最多2000项/2MiB；捕获恢复账本最多32项/64KiB，只存元数据，未完成会话变为中断/结果未知，不重放。损坏账本阻止新捕获；清理只影响捕获及其账本。真实 IPC、持久化和浏览器测试由同一 PR 验证记录支持；手机后台、磁盘不足、系统文件/分享仍需实际验收。

## PLATFORM-01

**平台与真机条件。** iOS 主应用/extension/App Group/允许 entitlement、移动 Host 采样器、插件执行与 SDK/敏感材料边界、发行安装路径和缺少许可材料仍按验收账本推进；平台不允许与尚未适配分别标注。没有付费 Apple 账号，不创建签名凭据，不合并或发布。

解除条件与验收：Galaxy S8/API24 及 iPhone 的实际设备和允许环境；前后台/锁屏/撤权/服务重建/网络切换/磁盘不足/文件或分享取消，以及 TCP/UDP/DNS/Fake-IP 与声明协议测试。浏览器、无签名 APK 或 UIKit SDK 库构建不替代这些结果。
