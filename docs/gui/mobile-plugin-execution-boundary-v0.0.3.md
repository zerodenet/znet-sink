# v0.0.3 插件执行与排空边界

实现位置为 `src-tauri/crates/plugin-sandbox/src/runtime/worker.rs`、`runtime.rs` 和 `src-tauri/src/services/plugins/operations.rs`。现有 `run`、页面 `invoke`、启动 hook 及 `invoke_scheduled` 都使用宿主持有的 `Executor`；兼容库入口也经过同一个进程级 broker。组件 Authority、client-core Capability Manager、SDK 服务及既有调度记录继续拥有业务授权、任务和副作用，worker 不添加订阅业务队列、重试或新调度语义。

## 准入与预算

全局最多 **2 个** VM/宿主 SDK 调用在途、**32 个**待执行请求。每个 Host 的 Executor 有独立关闭意图；不同 Host 共享全局预算，关闭一方不关闭另一方。线程按第一次提交懒启动，每次执行新建并销毁 QuickJS Runtime/Context，没有复用 guest 全局变量。满队列明确返回 `busy`，不默默增加线程或自动重试。组件从提交到结果发布始终持有唯一 busy lease，不能利用多个入口并发执行同一组件。

排队期限独立为 **5 秒**。同一个租约在实际 VM dispatch 时只激活一次执行期限：普通执行仍为原 manifest `timeout_ms`（1–2000 毫秒）的整体执行墙钟上限；scheduled 执行仍为原 **120 秒**墙钟上限。guest CPU 另外按 manifest 预算计量，原生 SDK IO 暂停 guest CPU 计量但仍受执行墙钟与每项 SDK `Call.budget` 的限制。激活不创建新租约、不改变 operation ID/授权 epoch、不延长 grant TTL，过期队列不能激活，重复激活拒绝。

堆（2–32 MiB）、栈（64–512 KiB）、调用数（1–32）及输出（1–64 KiB）继续使用已验证 manifest 的限制；普通输入 16 KiB、scheduled 输入 256 KiB，SDK 请求/结果与具体方法预算仍按既有 SDK 契约执行。`WorkerSnapshot` 仅含计数、关闭状态和全局/排队/执行预算元数据，不包含输入、结果、配置或权限秘密。

## 原授权和结果提交

宿主在现有 operation 锁内捕获材料 identity、registration、revision 与配置；Authority 核对原 registration，`Policy::begin_at_revision`/`begin_queued` 在同一 policy state 锁核对原 revision 并取得 lease。停止后再启动、撤权后重授权、同包重装均不能让旧审阅取得最新授权。页面调用使用原 document 的同一取消信号。

执行回执保留原 lease；VM/SDK 回调、VM 返回、普通 markerless 结果、命名空间更新的 owning operation 锁内以及最终交付均再次检查它。原 lease 未释放前同组件依然 busy。共享 SDK 仍检查实际 permission 与资源预算；OS 浏览器打开也使用原 lease，成功回执只表示系统接受请求，不代表浏览器呈现。

取消、撤权或 deadline 到达时，未 dispatch 的队列项被移除，即使所有 worker 正在等待原生 IO，也不需要等待空闲 VM 才处理取消。已 dispatch 的 VM 检查中断、拒绝后续 SDK 调用和晚结果。已经发出的网络请求、系统操作或正在提交的原子写入不能被宣称撤回；交付被拒绝后不自动重放、不假报回滚。

## VM 文件选择的未支持边界

所有 VM `run`/`invoke`/scheduled 的生产 SDK dispatcher 在读取 AppState 或执行 IO **之前**调用 typed `validate_vm_sdk_admissibility`。当前明确拒绝 `FileRead`/`FileWrite`，返回 `unsupported` 与可信文件选择未接入的原因。guest JSON 中的 `selected_path`、绝对路径、声称是 selection 的字符串和 `scope=user` 均不能创建宿主系统选择授权；错误不携带该路径或文件正文。该准入不关闭其他 SDK 方法，它们仍经过现有实际 permission/scope/budget 检查。

签名页面 PageFrame 的独立 RPC 使用宿主真实系统 picker 覆写路径，并沿用原 document lease；此路径不受 VM gate 影响。VM 文件能力只有接入原生选择器或宿主持有、不可伪造且绑定原 document/component registration/grant epoch 的 opaque selection ticket 后才可开放，还需在实际 IO 与交付前核对撤销/取消/期限；不能只删除当前拒绝分支或信任 guest 自报文件名。

## 关闭与故障

`Host::executor_shutdown_handle()` 返回本 Host 的可克隆生命周期 handle。`Executor::close(Duration::ZERO)` 非阻塞关闭准入、取消排队、拒绝晚结果；正值只在给定期限内等待本 owner，在途原生 IO 未结束时报告 `running/draining` 与 `drained=false`。计数中的 active slot 直到 VM 与回调实际返回才释放。共享空闲线程可以继续服务其他 owner；`drained=true` 不表示进程级 worker 线程已终止。

应用正常退出先关闭页面 document 和 executor，再有界等待 2 秒，随后执行既有内核/TUN 清理。异常系统终止不能承诺 Rust 析构或 SDK 撤回；恢复仍需重读持久意图及真实运行事实。

Rust adapter panic 经 catch-unwind 转成固定 `HostFailure`，回执不携带 panic 内容；guest 捕获 JS 异常不能消除宿主失败记录。调度 join 失败也返回固定文案。worker 可以处理后续任务，但不能把此行为称为原生进程故障隔离。

## 平台边界与验证

本实现是 **同进程 VM/API 隔离**，不隔离 QuickJS 或原生 SDK 的 native crash、OOM、内存漏洞或操作系统强制终止。要求 `Process` 隔离的插件保持拒绝。Android/iOS Target、SDK 平台注册与执行准入未开放，移动插件页面仍显示适配缺口；本改动不创建 VPN、移动原生执行实例或签名凭据。

回归覆盖共享并发/队列超限、独立 owner 关闭、排队撤权不 dispatch、在途真实 draining、原 lease busy 与 stop/start ABA、review registration/epoch、SDK 二次调用与晚结果取消、原生 callback panic 固定失败、普通 IO deadline 与 scheduled wall 区分、队列激活一次性与过期拒绝，以及现有包验证/权限/VM 预算/Process 拒绝测试。宿主回归直接调用真实 `run`/`invoke_reviewed`，检查关闭 gate、旧 review 拒绝、取消后的 markerless 结果和 namespace 写入拒绝。该测试范围不替代移动真机或原生崩溃/OOM 隔离验收。
