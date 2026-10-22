# 客户端嵌入式 Proxy 生命周期

`src-tauri/crates/mobile-runtime` 是客户端拥有的具体 Rust 运行所有者。它直接依赖 `zero-proxy` 的公共 Rust API，实际构建 `Proxy`、调用 `Proxy::spawn` 并持有 `RunningProxy`，不以独立 `EngineHandle` 控制状态代替网络运行，不依赖 C FFI。

本库尚未链接或注册到当前 Android/iOS 主应用、VpnService 或 PacketTunnelProvider；不把源码、Linux 回环转发或交叉编译视为移动 VPN、随包内核安装及 M01 全部验收。

## 公共 API 与所有权

内核及其原 vendor 输入固定为 `e74561f99acca941bfadf0d33e35a5ef08f34086`。依据该提交：

- `zero-proxy::Proxy::new(RuntimeConfig)` 构建实际 DNS、协议 registry 与 Engine。
- `Proxy::spawn()` 创建真实异步网络 orchestration，返回 `RunningProxy`。
- `RunningProxy::shutdown(self).await` 发出结束请求，等待 orchestration 与既有资源协调的实际回执。
- `ProxyHandle::new(EngineHandle::new(proxy.engine().clone()), proxy.clone())` 与内核应用的真实组合相同，QueryService/CommandService 保留 Proxy 控制分派；命令调用 `execute_acknowledged`。

`RuntimeOwner::start` 创建自己的 OS 线程和两线程 Tokio executor；owner 线程及两个 IO worker 均显式设置16MiB栈预算，不依赖测试进程的 RUST_MIN_STACK。构造完整 RuntimeConfig 的测试 harness 另按内核工程约定设置 RUST_MIN_STACK=16MiB；该测试设置不改变生产 owner 线程预算。`RunningProxy` 留在该线程，UI 仅取得可克隆 `ControlHandle`。丢弃 UI handle 或 UI 自身 executor 不停止转发；原生宿主必须保留 RuntimeOwner。取消 start 等待或丢弃所有者会发出同一结束意图，线程继续原来的清理，不假报已完成。

`Spawned` 仅表示公共 `Proxy::spawn` 已执行，不能表示监听已就绪、运行线程未曾异常退出、物理出口健康或 VPN 已接管。该固定公共 API 没有可直接访问的 readiness/JoinHandle 观察接口；本库不靠 Engine 的 healthy 布尔值编造 Proxy readiness。资源查询仍使用真实 ProxyHandle；启动后的异步 orchestration 错误最迟在实际 shutdown 回执中呈现 Failed。

停止先拒绝新的控制请求，等待已经提交的 acknowledged 操作，再调用实际 RunningProxy::shutdown。排队但尚未执行的请求返回 NotSubmitted；丢弃等待不等于撤回在途内核命令。`Disposing` 表示 orchestration 已退出而 native executor 尚在释放；只有 OS 所有者线程实际 drop executor 后才发布 `Stopped`。Tokio 的 shutdown_timeout 无法确认被遗留的 blocking 任务已经结束，因而不用它宣布资源全部释放。宿主可为等待设期限；超时应保留 Stopping/Disposing、结果未确认，原线程继续清理，不自动重放或盲目中止。

## 控制、权限和失效

控制通道最多64个排队请求，拒绝超量，不新增 probe/tool 业务队列。每个请求绑定实际 core_instance_id/config_revision，在串行分派点重新核对，实例或版本不符时拒绝；owner 从不暗中切到替代 Proxy。配置修改后的身份从真实 Engine 重读，并在交付原命令回执、唤醒等待者之前发布；错误回执也遵循同一发布顺序，不假定失败意味着运行版本没有变化。发布只更新身份，不覆盖并发宿主停止意图。查询仍核对调用者固定身份，不自动换新身份或重放写命令。停止回执也保留最后实际身份。新所有者使用新的内核实例，旧 handle 停止后不能对它执行命令。

本库是可信 native Rust API，AuthContext 由原生宿主校验并传入；该参数不是 guest/JS 自报的登录或授权凭据。它使用 zero-api 的 required_permission 和 AuthContext::allows 进行准入，与内核 IPC 命令的权限边界一致。公共 ProxyHandle::execute_acknowledged 没有 AuthContext/principal 参数，不假称向它传输了调用主体；可信调用者、平台权限、客户端 capability lease、授权撤销与结果交付检查仍须由现有宿主 bridge 组合。严禁直接将这些参数暴露为未鉴权 invoke。错误仅返回固定类别或 ApiErrorCode，不回显内核配置和错误正文。

## 原生 TUN 的边界

当前原生外部 TUN/packetFlow 与 socket protect/bind 未接入，故本库在所有 feature 组合、Linux 测试也拒绝：

- 初始 RuntimeConfig 中的 `runtime.tun`；
- `tun.start`、`tun.stop`、`tun.recover`；
- `config.apply` 或 `config.apply_runtime` 经真实 RuntimeConfig 解析后引入 `runtime.tun`。

该固定 schema 中 TUN 属于 RuntimeConfig.runtime.tun，InboundProtocolConfig 列表没有 TUN 变体；不发明另一份手机配置方言。手动宿主 shutdown 保持独立可执行，不把桌面 TUN 命令当作手机 VPN 断开。普通 TCP 回环转发没有 VPN 接管，不证明外层连接可绕过 VPN。

## 依赖和构建接线

此 crate 保持独立 `[workspace]` 与独立 Cargo.lock，主客户端 workspace 显式 exclude；默认 feature 为空，Direct TCP 是内核本身始终提供的能力。测试显式关闭默认启用的 inbound UDP，仅验收 TCP。Socks5/HTTP/Mixed/VLESS/Hysteria2/Shadowsocks/Trojan/VMess/Mieru/DNS/WireGuard 为原公共 Cargo feature 的透传选项，实际发行应固定特性并读取 capability，不把本测试选择当作生产协议范围。

Git 依赖不继承其原 workspace 的 `[patch.crates-io]`。本 crate 单独按上述提交引用原 fork 包：gotatun、h2、hyper、rustls、openssl-sys、blake3、quinn-proto、h3-quinn。首次直接使用 registry rustls 失败，因为固定内核要求 fork 的 legacy-client 特性；这些输入已由 crate manifest/lock 明确保存。它们不能直接覆盖当前桌面客户端的 TLS 依赖。zero-proxy→zero-transport 即便默认特性关闭也依赖 vendored OpenSSL/ring；交叉构建仍需要真实目标 C 工具链，iOS 构建需要 Apple SDK。

若后续原生运行宿主以 path dependency 引用该包，消费宿主必须显式采用同版本 vendor 输入（依赖包的 patch 不会自动生效），选择已验收编译特性，并持有 RuntimeOwner。主客户端本轮不添加这个 path dependency，不改变桌面子进程或 IPC 架构。Proxy 本库也不自动启动 root application 的 HTTP/gRPC API 或 Connector 服务；这些服务、配置应用的上层合成与运行确认仍需宿主按实际能力接入。未来若要合并 workspace，须移除本库 `[workspace]`/主 workspace exclude，处理 vendor patch 与桌面 TLS 输入隔离，并重建锁文件和所有目标验证；不能悄悄合并。

独立验证入口：

```sh
cargo fmt --manifest-path src-tauri/crates/mobile-runtime/Cargo.toml -- --check
cargo check --manifest-path src-tauri/crates/mobile-runtime/Cargo.toml --locked --all-targets --all-features
RUST_MIN_STACK=16777216 cargo test --manifest-path src-tauri/crates/mobile-runtime/Cargo.toml --locked --all-features
cargo clippy --manifest-path src-tauri/crates/mobile-runtime/Cargo.toml --locked --all-targets --all-features -- -D warnings
```

## 已有证据与未运行项

实际生命周期测试包括32KiB Direct TCP 回环转发（销毁原 UI executor 与 handle 后仍转发）、真实统计与模式查询/写命令、读取权限与错误实例拒绝、停止后旧请求拒绝及端口可重新绑定、实际异步 bind 失败返回 Failed、取消启动与宿主丢弃后资源清理、各个 native TUN 分派拒绝。连通性轮询只在发送应用数据前重试 connect，不重复发送正文制造成功。

Linux 默认特性实际生命周期5项通过。主代理已执行真实 Android NDK check；固定 core 在 `zero-tun/src/lib.rs:165` 的不支持目标分支 `create() -> io::Result<impl TunDevice>` 报 E0282，错误分支没有可推断的具体 TunDevice 类型。即使不启用 TUN/额外协议，Proxy 的该无条件依赖也阻止当前 Android owner 构建（CORE-05）。客户端没有改写 core、Cargo cache 或 vendor 副本规避该错误；修复需在 core 通用目标构建边界协调，仍不代表 external TUN/protect 已完成。证据：`/tmp/embedded-mobile-runtime-android-check.log`。

最终全特性 test、fmt/check/clippy、iOS 编译证据由 PR 验证报告分别记录。未完成实际 VpnService/PacketTunnelProvider 注册、跨进程 IPC 鉴权、外部 TUN、protect/bind、网络变化、TCP/UDP/DNS/Fake-IP 真机转发、签名发行或随包产品验收；iOS SDK 编译也不等于真 iPhone VPN 验收。

命令回执发布顺序回归使用实际 Proxy 和自定义同步 waker，在 oneshot 唤醒调用者的瞬间检查真实更新后的身份；旧顺序可稳定失败。另以32次真实模式提交、当前身份查询、旧 revision 拒绝及明确拒绝后的原配置核对检查交付契约。它们不依赖串行测试调度，也不放宽 core_instance_id/config_revision 前置条件。
