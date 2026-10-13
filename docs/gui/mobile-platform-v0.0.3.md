# v0.0.3 移动平台运行边界与交付缺口

2026-10-13。客户端基于最新 `develop` 实施，内核只读审计基线为
`zerodenet/core@e74561f99acca941bfadf0d33e35a5ef08f34086`。
本页说明此次客户端代码的实际边界；不把布局、共享 Rust 测试或桌面构建
视为 Android/iPhone VPN 验收。完整 PRD V3 的网络交付要求仍有效。

## 已接入客户端的防误执行边界

- `gui_mobile_platform_snapshot` 是已注册的只读命令，不请求系统 VPN 权限。
  Android/iOS 当前明确报告 `adaptationState=not-adapted`、
  `vpnSupported=false`、`bundledKernelAvailable=false`、`kernelState=unavailable`、`vpnState=unknown`，
  不生成内核实例，不把没有服务观测误当作 VPN 已关闭。
- `application::run` 按原生编译目标分开装配。移动 UI 不启动桌面子进程、
  桌面代理、托盘、UI 计时调度或退出时的内核清理。将来实际转发必须由
  Android `VpnService` / iOS `NEPacketTunnelProvider` 独立持有。
- 移动应用通过 Tauri 原生路径解析初始化唯一私有容器；共享存储服务在初始化
  前返回错误，不能回退到 `HOME`、`XDG_CONFIG_HOME`、桌面绝对路径或当前目录。
  本地 profile、订阅及规则存储继续复用现有 Rust 领域。
- 命令注册层在 Android/iOS 拒绝桌面进程控制、原始桌面 IPC、系统代理（其底层 mobile 分支也返回明确不支持，绝不生成关闭快照）、
  桌面 TUN 开关、桌面安装器、任意内核安装和路径打开。拒绝发生在执行前，
  返回 `mobile_platform_unavailable` 及原因。子进程启动和内核安装服务也有内部
  门禁，内部调用不会绕过注册层而创建这些资源。已有本地草稿与只读管理能力
  保留；需要运行态的命令仍须通过现有权限及可用性验证。
- Cargo 把 single-instance、process、updater 与 tray 功能
  限定到桌面目标；移动权限文件不授予桌面进程、托盘、更新器或桌面目录能力。
- `macos-private-api` 保持原有 Cargo feature 和配置，因为 Tauri 2.6.2 的
  manifest 检查忽略 root dependency 的 target additive feature。这一 feature
  只在 macOS 兑现，Android/iOS 不因此获得 macOS 系统 API；移动原生适配仍不可用。
- Android 配置显式冻结 `minSdkVersion=24`（Android 7.0）。Galaxy S8 是必须
  验证的设备；API 下限和响应式页面不证明该设备 WebView、包构建或 VPN 已兼容。
  iOS 保留 Tauri 自身系统版本基线；本批没有另行冻结产品最低 iOS 版本。
- 平台配置禁用桌面 updater artifacts。移动内核策略为随应用发布；此次未包含
  已链接的移动内核，也没有开启独立原生代码下载、任意本地执行或模拟安装。

`znet-client-core::mobile::reconcile` 是移动宿主观测的共享准入规则：
只有完整 Proxy 正在转发、外部 TUN 已接入、物理出口保护、DNS 已确认、
平台授权/隧道和物理网络状态均满足，且实例、profile、config revision 一致，
才可投影 `taken-over`。平台撤权独立投影；没有平台采样是 `unknown`。
当前生产快照以缺少观测调用这一规则，并保持不可用。
共享恢复票据模型固定用户意图 revision 和完整运行身份；停止/撤权/再次启用
或实例切换使旧票据失效。它尚未接入真实移动服务，因为本批没有该运行桥，
不能据此宣称自动恢复或服务持久化已完成。

## 最后审计：本地保存与未适配系统 IO

移动注册层还明确门禁模式/策略选择、probe/tool job 启动、运行统计、端点、Packet、
连接、事件、运行态校验与工作区提交；这些动作不能通过 `default_opts` 访问桌面 IPC。
本地 client/node/profile/settings/workspace 快照和本地 CRUD 保留。节点页的原生
移动投影仅使用配置骨架，报告运行观测不可用，不访问桌面 policy query；C01 导航
能力投影也不会复用或查询桌面 core capability 缓存。任务历史仍可读，不能启动
一个只有 UI 寿命的移动任务。DNS 离线保存继续使用共享配置事务，并报告未应用。

普通配置创建、编辑、激活和删除的本地发布，不要求存在桌面系统代理：
`desktop_profile_effect` 在原生移动端返回明确的“没有执行桌面效果”（`None`），
不调用桌面 ownership 读取/retarget/compatibility。底层系统代理 API 仍返回不支持，
这里没有伪造 OS 操作成功或 VPN 已关闭。移动主动配置标记表示本地选择；运行生效
仍须等待未来服务桥核对，未实现全量编译协议预检的本地导入不算 A08 完整通过。

A08 仍为部分完成：粘贴内容沿既有配置材料准备路径保存并保留字段；系统文档选择
授权与 `content://`/iOS security-scoped 内容读取适配尚缺。任何没有 content 的
path-only 导入在创建 file lease 或读取文件之前返回
`mobile_file_import_not_adapted`，给出粘贴内容替代入口。传入 content 时不会借同时
传入的路径扩大授权。系统文件导入、保存/分享、取消/拒绝回执及实际手机验收仍未完成。

## 阻塞真实转发的上游通用接口 M01–M04

审计源码与需要协调的最小通用范围如下。没有修改或推送 core。
这些工作属于内核的已有职责抽象，不应包含客户端订阅、插件、商店或安装业务。

| 依赖 | 当前真实源码 | 需要 core 协调的范围 | 客户端后续接入 |
| --- | --- | --- | --- |
| M01 完整运行生命周期 | `crates/ffi/src/lib.rs` 的 `zero_start` 仅 `EngineHandle::from_path`；`crates/proxy/src/runtime.rs` 已有 `Proxy::new/from_path/spawn`，`runtime/running.rs` 已有 `RunningProxy::shutdown` | 提供可嵌入的完整 Proxy handle / 受支持移动 FFI：创建、启动、启动结果、ProxyHandle 控制、停止并等待所有任务释放、失效句柄拒绝。`EngineHandle` 保持控制职责；不能重命名其状态为网络正在运行 | 原生服务/扩展持有 runtime，UI 只使用控制代理；核心随包链接，显式停止由宿主协调 |
| M02 外部 TUN | `crates/tun/src/lib.rs` 已有 `TunDevice::into_channels[_observed]`；`crates/proxy/src/inbound/tun.rs::start_tun_internal` 仍调用 `zero_tun::create` 和桌面网络配置 | 在 `tun`、`proxy/inbound/tun` 与已有 `stack` 接入正式外部设备/packet channels。复用现有 stack/proxy 路径、方向、IP family、MTU、背压、关闭、真实设备边界统计；OS 已配置接口不能再次走桌面 route/DNS 创建 | Android service 建立并持有 FD；iOS provider 持有 packetFlow。主 UI 不创建 FD，不解析 IP 包或另写转发器 |
| M03 外层出口与网络变化 | `crates/platform/tokio/src/egress.rs` 有桌面接口绑定；`crates/platform/tokio/src/lib.rs` 有 TCP/UDP 构造；现有 `DialPolicy` 位于 traits | 在 traits/platform 现有 socket 构造边界注入宿主保护/绑定，必须覆盖 TCP、UDP、DNS、QUIC及协议外层 socket，发生在连接/发送前，拒绝保护失败。引入真实物理网络身份/代次、IPv4/IPv6 出口变更和失效池/拨号的协调；不把 Linux 接口绑定当作 Android protect | Android `VpnService.protect` 与 `Network.bindSocket`/底层等价 API 及物理网络回调；iOS 使用 provider 允许的外部连接和路径变化，不臆造跨平台 FD API |
| M04 控制和事件桥 | `crates/proxy/src/runtime/handle/{model,query,command,event}.rs`、`zero-api` 请求/回执/事件已存在；桌面 GUI 使用私有 IPC | 完整 runtime handle 上提供类型化异步查询/命令及有界事件订阅、取消与间隙信号；保留真实 core_instance_id/config revision/权限，不依赖桌面 endpoint 文件或把秘密放入普通事件 | 客户端宿主负责进程/扩展边界鉴权、用户权限投影、服务状态通道和持久恢复记录；前台重新发现实例及 cursor，不接受旧结果控制新实例 |

接口名称与具体 JNI/C/Swift 表面应在上游评审冻结；表内是所需语义，
不是客户端已实现了这些 core API。公共 API 继续使用 `zero-api` 的 schema、
capabilities、permissions、资源身份和回执。配置、协议私有解析、Packet 路由、
WireGuard 认证及 Peer 语义保持原有 core 分层。

客户端可独立完成布局、现有业务页面投影、本地配置管理、只读资料与不可用原因；
缺口影响移动 runtime 和真实系统 IO 接入，不能用桌面成功代替。
没有添加未接入的 Kotlin/Swift 服务骨架或另一个 EngineHandle 数据面。

## 给独立 core 云端任务的 API 建议与退出清单

以下是评审建议，不是新增或已经可调用的 API。建议以 Rust 内嵌接口为主，
C/JNI/Swift 只包装同一 owner；不要重建第二个 mobile engine 或路由器。

### M01：单一完整 runtime owner

在 `crates/proxy/src/runtime/{running,handle/model}.rs` 提供受支持的嵌入 owner，
将 `Proxy`、来自同一 Engine 的 `ProxyHandle` 和 `RunningProxy` 绑定。建议形状为
`EmbeddedProxy::start(RuntimeConfig, HostRuntimeOptions) -> Result<EmbeddedProxy>`、
`control() -> ProxyHandle`、`readiness() -> RuntimeReadiness`、
`shutdown(self) -> async Result<ShutdownReceipt>`。名称由上游确认。

- `start` 成功只表示 owner 已创建；只有 orchestration ready、所有需要的 listener/
  device 初始化回执和平台隧道确认完成，才是可接管状态。启动失败须释放此前资源。
- `shutdown` 先拒绝新操作、停止接收设备输入、关闭 listener/调度，再终止/等待
  forwarding 及 I/O driver；返回明确资源释放结果，不能只发送 shutdown 通知。
- UI/查询 handle 被 drop 不停止 owner；service/provider 的明确关闭销毁 owner。
  FFI handles 具有不可复用身份，close 后所有旧查询、命令及回调必须拒绝。
- FFI `crates/ffi/src/lib.rs` 可保留旧控制-only API 兼容，但新增完整运行 API 应
  清楚区分；句柄内保留 tokio runtime 和完整 owner，不再以 EngineHandle 顶替。
  配置优先从受校验内存材料构造，不要求移动 UI 导出临时普通 JSON 文件。

退出测试：真实 Proxy TCP/UDP/DNS 转发；重复启动/关闭；启动中撤权；部分 listener
失败资源清理；关闭等待 driver；服务重建产生新的实际 instance；旧 FFI handle
不允许再控制。启动或 stop 的不确定回执通过只读重查核对，不能自动重放。

### M02：外部设备由平台持有，既有 Proxy 消费

建议在 `crates/tun/src/lib.rs` 提供公开但窄的外部设备/packet port 类型，并在
`crates/proxy/src/inbound/tun.rs` 增加与 `start_tun` 共用 stack 初始化和运行逻辑的
外部入口。既有 `adopt(fd, interface)` 只在 Linux/macOS 编译，且有桌面接口校验，
不是 Android 可直接消费的公共入口。`TunPacketSender/Receiver` 当前是私有 type
alias，也不能把名称存在当成外部注入已经完成。

- 建议 `ExternalTunPort` 明确 `from_host`（系统进入 Proxy 的原始 IPv4/IPv6 包）、
  `to_host`（Proxy 返回系统）、协商 MTU/地址族、设备 lease 和 close/错误回执。
  核心只消费 raw IP，不包含 Apple packet-family header 或猜测 FD 包格式。
- Android 由 service 持有 `ParcelFileDescriptor`；桥使用明确独占转移或 dup 语义，
  不允许 native 与 Java 分别关闭同一 FD。iOS provider 始终持有 packetFlow；核心
  持有端口租约，扩展负责原生读取/写入及释放。
- 外部已建立设备不再调用桌面 `create/configure_addresses`、系统 route/DNS 事务。
  平台将实际网络设置确认传入 runtime，不能把请求的网络设置当作生效结果。
- 有界包队列同时限制条数和字节。建议初始预算每方向至多 256 包/4 MiB、单包
  至多 65535 字节；具体值与已有 UDP storage policy 一起确认。条数限制本身不能
  限制 Vec 总内存。背压允许挂起/暂时拒绝，不记录为最终丢包；最终丢弃使用既有
  `PacketDropReason`。队列观察压力不能阻断 forwarding。
- 成功接收/实际设备写入边界复用 `IoObserver`，队列入队不能算真实发送；close
  必须唤醒阻塞 read/write/队列等待，拒绝后续包并返回退出回执。包方向、IPv6、MTU
  和所有已编译协议继续进入现有 `UserNetworkStack`/Packet 路径。

退出测试：socketpair/有界端口驱动 IPv4/IPv6 双方向；MTU 限制；关闭/读写错误；
队列饱和内存上界；统计只在真实 IO 边界累积；Android FD 和 iOS packetFlow 真机
接入既有 stack；停止后无后台 worker、FD 泄漏或旧设备统计写入新 instance。

### M03：所有物理外层 socket 在第一次网络 IO 前受保护

在 `crates/traits` 保持 runtime-neutral 的宿主外网能力/受保护 socket factory；
实现和平台句柄封装留在 `crates/platform/tokio/{lib,egress,dial}.rs`。不要把 JNI、
Swift 对象或桌面接口名称写入 config/engine，也不要让协议逐个实现 Android protect。

建议 immutable `HostNetworkLease { network_identity, generation, families }` 与
`HostSocketFactory`/`HostSocketPolicy` 的窄边界。native FD 平台可以在创建后保护；
没有这一原生句柄模型的平台通过 provider 支持的连接 factory 兑现同一合同。
协议消费者只收到已保护的 socket/stream，且保留其平台 network generation。

执行顺序：读取有效物理网络 lease → 创建尚未连接的 socket → 平台 VPN 排除
（Android protect）→ 根据允许策略绑定物理网络（Android bindSocket/等价 API）
→ 校验 lease 未退休和目标 family/DialPolicy → connect/首次 send → 协议握手。
保护/绑定失败、network generation 变化或 family 无出口均终止这一拨号，不能退回
未保护 socket。TLS/QUIC/UDP/系统解析/节点解析/探测、端点外层和端口轮转均覆盖，
包括当前直接 `TokioSocket::connect` 的路径；只修 direct TCP 不满足合同。

平台网络变化更新 egress selector 和租约代次，退休不再可用的连接池/UDP association
及 DNS egress 缓存；在途旧拨号无法向新代次提交结果。恢复时分别读 IPv4/IPv6，
保留“无网络出口”事实。不要以某一个 Wi-Fi 接口存在或一个 socket protect 返回值
确认远端健康，也不要声称 strict route 防泄漏直到平台能兑现对应保证。

退出测试：替代宿主记录 protect/bind/connect 的严格顺序；protect/bind 拒绝没有
网络 IO；每种 socket factory 的保护覆盖；代次切换时旧池/拨号拒绝交付；真机
Wi-Fi↔蜂窝、IPv6-only、断网/恢复、其他 VPN/撤权无外层回送和假健康。

### M04：复用现有控制合同的窄桥

已有 `zero-api::{QueryService, CommandService, EventSource}` 与
`ProxyHandle::execute_acknowledged`、`EventSource::{subscribe,since}` 应复用。
需要补的是完整 runtime 所有权与移动桥，不是新的移动 Query/Command 方言。
通用 FFI/embedding 桥属于 core；跨进程 Android binding/iOS app-extension channel
鉴权、应用持久记录和 UI 权限投影属于客户端宿主。

- 请求固定实际 core instance、当前 config revision、resource identity 和调用权限，
  控制回执来自所属 Proxy；不允许客户端计数代替 core instance。native 控制通道
  绑定 service/provider session、安装身份及调用方，外部页面不能直接拿 admin。
- `AuthContext` 在 dispatch 前检查 `required_permission`，区分 Read/Control/Config/
  Admin；系统 VPN 授权不授予其他 core 或插件权限。一般查看不走平台授权。
- 当前 `zero-api::ConfigApplyCommand` 仅携带 `config`，`ModeSetCommand` 仅携带
  mode/outbound，`PolicySelectCommand` 等也没有预期 instance/config revision。
  客户端已有操作锁及读前/读后身份核对可以阻止同一客户端的旧页面动作，却不能
  原子阻止另一个控制器在读取与提交之间修改内核。M04 还应评审兼容的可选
  preconditions 扩展，优先为配置、模式、选择及作用域受影响的 probe/write 增加
  expected instance/revision，并由 owning core 命令在同一协调锁内校验。
  旧协议没有 CAS 时明确受限，不把客户端序列化或 after-read 当作内核原子保证。
  本批移动 runtime 未开启，因此不能宣称 E06 的完整远端并发验收已完成。
- JNI/Swift 请求异步关联 operation ID，timeouts 表示结果未知；取消 UI listener
  仅终止这一 listener，不能重放写操作、取消已发网络请求或停止公共 runtime。
- 建议事件 bridge 限制 256 帧/1 MiB、单事件 64 KiB、最多 8 个 UI 订阅；配置材料
  独立受限传输，不放事件中。这是待协调初始预算；已有 core event/replay 上限
  仍是能力准入的一部分。溢出显式报告 gap，不给生产者无限队列或阻塞数据面。
- 事件绑定 instance、subscription generation、cursor；前台恢复/进程重建先重取
  当前能力和快照，再用已有 replay/gap 语义接回，旧页面回调和旧 instance 结果
  失效。取消订阅释放缓存与 channel，退出观察页不停止公共事件源。
- RuntimeConfig、URL tokens、凭据和插件材料不进入一般事件/错误/诊断。受保护
  材料加载须使用已验证的内存或不透明材料路径；不能借普通临时文件兜底。

退出测试：权限拒绝发生在副作用前；实际 instance/profile/revision 换代拒绝旧控制；
operation 丢回执仅重查；listener 取消/应用退出不终止 service；事件溢出报告 gap 并
重同步；序号/大整数无损传输；不信任 caller 自报身份；敏感数据不进入事件与错误。

独立 core 任务应依该仓库 `AGENTS.md` 完成 workspace fmt/check、
`RUST_MIN_STACK=16777216 cargo test --workspace --all-features`、
`cargo clippy --workspace --all-targets`，并补各 owning crate 的行为测试。
上述通用出口接口通过后，客户端原生宿主任务再接 JNI/Swift、系统 IO 与平台验收。
不能把这份建议、替代端口测试或 C 编译通过作为最终手机 forwarding 退出条件。

## 平台验收与发行约束

Android 后续需要：`VpnService.prepare` 授权；正确声明的 VPN service 及必需前台
通知；建立 FD 后接入上述运行桥；主动关闭先取消恢复意图；系统撤权、其他 VPN、
网络断开及 Wi-Fi/蜂窝变化独立核对；UI 消失不终止 forwarding。
API24 兼容不能使用未门禁的新平台 API。至少覆盖 Galaxy S8、双栈/IPv6-only、
TCP/UDP/DNS/Fake-IP、后台/锁屏、低内存与服务重建。当前未执行这些设备用例。

iOS 后续需要：Packet Tunnel extension、有效 Network Extension entitlement、
provider 网络设置/packetFlow、允许的 App Group/容器及主应用控制通道。
本批没有付费开发者账号、签名凭据、Xcode/macOS 构建或真 iPhone VPN 验收。
不会购买或创建签名；只有依用户后续授权及可用环境才能进行对应验证。
商店更新和绑定内核与 desktop updater 分开，插件容器/分发及保护材料仍需
按真实平台限制验证；插件固定入口不得被删掉或显示伪造运行结果。

有限本地调度不能作为系统后台任务；需要运行态的 probe/tool jobs 应在服务侧
复用已有任务队列和恢复记录。停止 probe 仍是停止排队并等待在途，无强制撤回承诺。
Host interface 统计、系统文件保存/分享、通知结果和插件 SDK 平台 IO 都须明确
设备支持，任何静态按钮或配置文件不算真实完成。

## 验证入口与证据

- `cargo test --manifest-path src-tauri/Cargo.toml --locked -p znet-client-core`：
  共享生命周期、身份、撤权、物理出口/DNS缺失、旧恢复票据及命令门禁回归。
- `pnpm verify:backend` 或工作区格式、check、test、clippy：证明客户端桌面及
  共享代码回归；结果由本次 PR 检查记录补充，不能当作手机包或网络验收。
- 本次共享层 Linux 测试 53 项通过（包括 11 项新增移动规则测试）；共享 crate
  clippy `--all-targets -- -D warnings` 通过。`aarch64-linux-android` 和
  `aarch64-apple-ios` 的 `cargo check --locked -p znet-client-core` 均通过；它们只
  验证无 Tauri/原生 IO 依赖的共享 Rust，不验证主应用、service/provider 或设备。
- 云端 NDK r27d `27.3.13750724`、`aarch64-linux-android24-clang` 下，
  `cargo check --manifest-path src-tauri/Cargo.toml --lib --target aarch64-linux-android
  --locked -j 2` 已通过，保留全部默认客户端工具 features。日志：
  `/tmp/mobile-android-host-check.log`。这一检查编译主宿主 Rust 与 native C 依赖，
  仍不是 APK/签名/安装/FD 接入或转发验收。40 项 warning 主要为移动装配不调用的
  既有桌面代码，未以忽略错误或返回伪造成功通过检查。
- `.github/workflows/mobile-host.yml` 新增同样 API24 Rust host CI 与两平台纯共享
  crate 目标检查，上传 Android 检查日志。CI 真实结果以本 PR 的 commit 检查为准；
  源码中的 workflow 本身不代表该 commit 已在 CI 通过。
- 后续 Android native build 需 Android SDK/NDK 和生成/维护的原生工程；iOS需
  macOS/Xcode。当前仓库没有生成的 `gen/android`/`gen/apple` 项目；尽管上述 Android Rust host
  target check 已通过，本批没有
  声称完成移动包构建、签名、安装、真实设备协议转发或 Galaxy S8 验收。

依据：[Android VPN 官方说明](https://developer.android.com/develop/connectivity/vpn)、
[NEPacketTunnelProvider](https://developer.apple.com/documentation/networkextension/nepackettunnelprovider)、
[NEPacketTunnelFlow](https://developer.apple.com/documentation/networkextension/nepackettunnelflow)、
[Tauri Android 最低版本配置](https://v2.tauri.app/reference/config/#minsdkversion)、
[Tauri 平台权限文件](https://v2.tauri.app/security/capabilities/#target-platform)。
