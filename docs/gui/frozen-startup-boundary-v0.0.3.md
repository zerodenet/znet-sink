# 冻结启动材料与真实启动确认边界

本说明核对客户端 `d8e6ac1` 与只读 Zero 源码
`e74561f99acca941bfadf0d33e35a5ef08f34086`。以下新增接口均为建议，
不是现有 capability 或已实现的启动回执。本次审计没有修改或构建 core。

## 当前启动链实际执行什么

| 位置 | 已有行为及限制 |
| --- | --- |
| `src-tauri/src/runtime_host/start.rs:23–28` | `prepare_bootstrap` 写管理专用空配置，再读取 AppConfig 中的启动设置。 |
| `src-tauri/src/services/core_config.rs:72–105` | bootstrap 不含来源正文；后续 `runtime_candidate` 重新读取活动 profile、客户端覆盖及规则，`apply_active_runtime` 执行运行态应用。 |
| `src-tauri/src/runtime_host/spawn.rs:128–137` | 校验 child 控制面就绪后，调用上述当前材料热应用；不是用冻结候选直接构造 Proxy。 |
| `src-tauri/src/models/core_config.rs:7–20` | `CoreConfigSnapshot` 保存可执行路径、配置路径、目录、endpoint 和参数，不保存候选 JSON、依赖字节或启动确认。 |
| `src-tauri/src/runtime_host/monitor.rs:27–30,281–292` | watchdog 保留路径快照，在配置操作锁及 host transition 下复用同一 spawn；其热应用仍会重读当前材料。 |
| `src-tauri/src/commands/core_process.rs:112–132` | 普通重启协调现有 TUN 意图及恢复错误；不提供工作区冻结材料与来源 CAS 回执。 |

现有 owned-child、parent-lifetime stdin、停止/wait、代次使 watchdog 退出及共享操作锁
都可以继续复用。它们不能证明某份旧工作区候选在新进程中生效，也不能让失败后的
整表恢复覆盖后来的来源关系。工作区重启继续保留影响与不可用原因；内核管理普通重启
是独立功能。

## 现有 core 不能提供的确认

固定 core 的实际调用顺序如下：

1. [`src/application/run.rs:47–56`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/src/application/run.rs#L47)
   从路径加载配置，预取 URL 规则，再创建 Engine、Proxy 和应用服务。
2. 同文件 `72–103` 先创建 IPC，再创建受编译能力约束的 HTTP/gRPC 服务。
3. 同文件 `107–113` 随后进入受监督的 Proxy 数据面运行。

客户端 [`readiness.rs`](../../src-tauri/src/runtime_host/readiness.rs) 的检查是稳定一段时间的
`health.healthy` 及 `runtime.pid == child.id()`。它证明控制面属于原 child，不能证明所有
listener、API 服务及启动字段完成初始化。core 的
[`engine/api/mod.rs:93–102`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/engine/src/api/mod.rs#L93)
直接生成 `healthy: true`；ProxyHandle 仅补充可用的设备健康，没有通用 Proxy startup ack。

[`ConfigSnapshot`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/api/src/snapshot.rs#L20)
只有 revision、mode、rule count、listener、outbound 和 group 投影，不包含完整原生配置或
完整摘要。`RuntimeSnapshot` 只有实例、revision、PID、config path 及部分运行字段，也没有
`api.control`、quota 状态路径或其实际初始化结果。返回配置路径不是确认读取了某份字节。

另外，
[`ApplicationServices::validate`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/src/application/services.rs#L229)
明确拒绝 live apply 改变 `api.control`；
[`reload_config_inner`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/engine/src/runtime/configuration.rs#L89)
明确拒绝 live reload 改变 `runtime.principal_quota_state_path`。因此“空 bootstrap 启动成功，
随后热应用”不能作为这些字段已生效的替代证明。

## 客户端仍可独立实现的部分

专用 frozen-start 应从预检阶段携带完整普通候选 Value、源/关系图/覆盖/规则摘要、持久
领域 epoch、host session、原 scope/generation、可执行文件身份、固定参数/endpoint/目录及
启动 operation ID。提交前重新核对原条件，先持久记录未知窗口，再停止原 owned child。
启动期间不先修改活动来源，不调用重读 current state 的 `apply_active_runtime`。

私有启动材料须排他创建、验证写入摘要并保持到 child 及相关恢复窗口结束。Unix 目录及
文件应在创建时收紧访问；Windows 需创建时的私有 DACL、所有者与拒绝 reparse/不安全
路径验证，并在实际 Windows 上测试。普通 `write_core_config` 的 `fs::write` 没有这些保证；
`atomic_file` 的原子发布也不等于已验证 ACL。材料不得进入诊断正文、URL、invoke 参数或
命令行原文。插件托管、受保护材料仍不能降级到普通 JSON 文件。

冻结 JSON 还必须处理真实依赖：

- [`RuntimeConfig::load_from_path`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/crates/config/src/model/mod.rs#L51)
  用配置父目录解释相对凭据、状态和产物路径；移动 JSON 文件的位置会改变语义。
- [`pre_fetch_rule_sets`](https://github.com/zerodenet/core/blob/e74561f99acca941bfadf0d33e35a5ef08f34086/src/rule_set_fetch.rs#L19)
  可重新下载 URL 规则并回退旧缓存；typed 规则集摘要不封存实际文件。
- 规则文件、GeoIP、TLS/CA/证书/私钥、环境凭据、quota 恢复文件、outbox 等需按原生
  typed schema 区分输入材料、运行输出和授权范围。网络 URL 的 `path` 不是文件路径，
  不能靠通用 JSON key 扫描猜测依赖。

可逐项封存并使用明确的材料清单，或拒绝尚未封存的范围并保留原因；不得静默清空路径、
下载新规则或把依赖换成默认值。变换为私有绝对路径时，需显式保存源候选与实际 launch
候选的映射，分别校验两份摘要，不能声称字节完全相同。

Host 应持有材料生命周期与原 launch 身份。pending/unknown 时的 watchdog 与下次
auto-start 必须遵守原操作屏障，不能自动换用当前来源、重新 apply 或重放该 operation。
失败只释放本操作仍拥有的 child/资源；没有权威旧运行材料时不自动应用本地旧配置。
确认之后的本地发布继续使用现有精确领域 CAS，失败保持“运行确认、本地待发布”分离。

这些属于客户端职责，不能全部归为 core 阻塞；仅完成文件或进程阶段时，应准确报告
该阶段，不能生成工作区 `confirmed`。本轮不引入未接生产链的空 launcher。

## 最小待协调的启动确认契约

建议由实际 Proxy 生命周期在真实启动完成之后提供非敏感的 startup acknowledgement，
内容至少关联本次 launch nonce/operation、core instance、初始 config revision、实际加载并
规范化的候选摘要、已声明的依赖摘要/确认范围，以及 listener/应用服务启动结果。
摘要应基于实际读取材料，不是把客户端传入的摘要原样回显；不能返回秘密正文或环境值。
该契约当前不存在，具体 wire 名称与 capability 应由 core 维护者确定。

若仅能确认某一组字段/依赖，必须明确范围；没有确认的 startup-only 字段仍不可报告
完全生效。客户端读取 ack 前后须保持同一 owned child、绑定连接与实例；后续运行变更、
重启或来源变化使旧 ack 只作为历史。启动命令的未知结果不能通过自动换 ID 再发一次。

验收至少包括实际发布二进制的下列反例及成功例：

- 配置未读、解析失败、依赖被替换、URL 缓存变更、凭据加载失败、listener/API bind 失败。
- 仅 Engine/IPC 可用时绝不产生 Proxy ready；错误 PID、旧实例/nonce、晚到 ack 均拒绝。
- 冻结普通材料直接启动，实际 TCP/UDP/DNS 转发，以及受支持 API、quota 初始化事实。
- Unix/Windows 的私有创建、权限失败、symlink/reparse、崩溃及磁盘失败；不泄露正文。
- watchdog 与 auto-start 不越过 pending 屏障，停止不会复活旧操作，本地 CAS 冲突不覆盖
  外部新关系；unknown 无自动重放。

这些证据与移动原生 VPN、签名发行和真机验收仍分别记录。
