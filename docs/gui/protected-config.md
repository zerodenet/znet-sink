# ZBoard 受保护配置与客户端内存运行设计

状态：待后续评审，尚未实现。2026-09-08。首个面板确定为 **ZBoard**。当前优先完成 [内部模块化与稳定性](./internal-modules.md)；本文仍保留此前的隔离执行和面板发钥设计，不代表必须实现动态插件，后续可改为内部受保护模块。

## 1. “配置不可读”的交付定义

首期保证受保护配置不进入用户可读取的配置正文、订阅链接、导出文件、SQLite 明文、日志、诊断包或未获敏感授权的插件内存。面板到客户端传输密文，插件敏感组件在自己的隔离执行域短时解密，Zero 在内存中消费运行配置；宿主不接收配置密钥或明文。UI 只拿宿主允许的节点别名、状态、延迟和用量。

这不是对本机管理员、调试器、被修改的客户端/内核或内存转储的绝对保密承诺。协议连接时，内核必须使用节点地址和认证材料；运行期间不能把全部有效状态始终保持加密。加密内存缓冲区、清零和锁页可以缩短暴露窗口，但不能消除运行时明文及 OS 网络观测。

| 对象 | 首期保证 |
| --- | --- |
| 普通客户端 UI、插件、配置导出 | 无完整配置/凭据读取入口 |
| 客户端持久数据、临时文件、备份、日志 | 应用不主动写入受保护配置明文 |
| 网络旁观者、被复制的缓存 | TLS + 密文信封；没有面板下发的 CEK 不能直接解密 |
| 本机管理员、调试/注入、修改程序 | 不承诺不可提取；防篡改和设备证明需额外体系 |
| swap、休眠、系统崩溃转储 | 尽量减小风险；不能等同于应用不写文件的保证 |

不能把 base64、混淆、静态内置 AES key、只禁用导出按钮或在 SQLite 加密后又导出启动 JSON 当作达标。

## 2. 已核对的现状

客户端基线 `6d822fb`；Core 本地源码 HEAD `50322956`；ZBoard HEAD `e1b7246`。Core 存在其他任务的 HY2 工作区修改，ZBoard 存在未跟踪的服务端插件设计草案，本次只读核对，不改动这些工作。

| 路径 | 当前事实 | 必须调整 |
| --- | --- | --- |
| GUI `models/proxy_config.rs`、`services/app_database.rs` | `ProxyConfigProfile.content` 可序列化，profile 写入 `payload_json` | 受保护配置使用独立类型，不把明文塞入普通 content |
| GUI `services/core_config.rs::export_active` | 组合配置并写入启动 JSON | 受保护来源禁止经过此出口 |
| GUI `services/core_process.rs` | 启动现有 active profile 时先 export | 增加受保护来源启动分支与无秘密 bootstrap |
| GUI `kernel/protocol.rs`、`models/debug.rs` | IPC 请求/响应正文进入 debug ring 和 `debug_store::append` | 受保护消息在捕获之前改为固定元数据，不先 clone 明文 |
| GUI `services/config_apply.rs` | 已核对应用前后内核身份/版本，不确定结果不重放 | 复用确认语义，增加 runtime-only 提交入口 |
| Core `crates/engine/src/runtime/configuration.rs` | 普通 reload/stage 可写源文件；runtime 版本 `persist=false` | 受保护实例锁定不持久化，不允许后续操作降级写出 |
| Core `crates/proxy/src/runtime/handle/model.rs` | 已有 `apply_runtime_config_and_wait`，等待 listener 协调 | 复用事务，不再实现一套面板专用配置引擎 |
| Core `crates/engine/src/api/export.rs` | `ConfigSnapshot` 是投影，不是完整原始配置；仍包含节点 server/port | 增加受保护观测策略，覆盖全部查询和事件出口 |
| ZBoard `backend/internal/server/router.go` | 已有登录、me、订阅访问管理及 token 订阅接口 | 新增加密下发契约，并封住受保护订阅的明文旁路 |
| ZBoard `subscription_delivery.go`、`subscription_export_formats.go` | 已有 ZNet Sink/Zero 格式选择和渲染 | 在权益校验和既有渲染后封装密文，不重新写协议导出器 |

上述 runtime-only 能力是当前源码事实，尚未验证目标用户安装的 Core 是否具备全部受保护能力。上线以配套发布物和 capability 握手为准。

## 3. ZBoard 服务端契约

首个插件 ID 定为 `org.zerodenet.zboard`，面板实例由用户配置 HTTPS origin。客户端插件包与 ZBoard 服务端 `.zbplugin` 草案不是同一包格式；本功能不依赖服务端动态插件框架先落地。

### 3.1 复用与新增接口

| 接口 | 当前/拟议 | 用途 |
| --- | --- | --- |
| `POST /api/v1/auth/login`、`GET /api/v1/auth/me` | 已有 | 宿主执行登录、识别账号；响应 token 不发给插件 |
| `GET /api/v1/subscriptions` | 已有 | 复用用户授权过滤后的订阅/套餐信息，实施时审核返回投影 |
| `GET /api/v1/client/protected/capabilities` | 拟新增 | 信封版本、算法套件、最大大小、租约策略、可信签名 key ID |
| `POST /api/v1/client/protected/sessions` | 拟新增 | 经用户认证建立客户端会话，绑定实例/账号/订阅、loader 会话和 challenge，签发最小权限会话 token |
| `POST /api/v1/client/protected/config` | 拟新增 | 校验会话与权益，生成配置、加密、签名，返回信封 |
| `POST /api/v1/client/protected/key-grants` | 拟新增 | 校验会话、信封摘要与权益，短期下发配置 CEK |
| `DELETE /api/v1/client/protected/sessions/:id` | 拟新增 | 撤销本客户端会话，客户端同时清理本地运行态 |

客户端会话 token 只允许本账号的公开套餐查询、受保护配置/密钥授权请求与会话撤销，不继承管理员权限或所有账号 API。登录 token 仅在可信宿主短时使用以换取会话；需要持久登录时，宿主凭据库仅保存可撤销、受限的续期凭据。

新增配置端点走既有权益、到期、用量、节点权限过滤和 Zero renderer；输出在 handler 内存中加密，禁止响应/错误正文日志。反向代理/APM 也不得捕获登录与配置请求体。加密签名 key 从服务端秘密存储加载，与节点凭据加密 key、JWT key、插件包签名 key 分开轮换。

### 3.2 必须关闭明文旁路

ZBoard 增加订阅级 `delivery_policy = standard | protected_only`。新受保护下发流程只服务 `protected_only`；该策略由服务端授权决定，不能由 User-Agent、format 参数或客户端布尔值证明。

受保护订阅的旧 `GET /api/v1/client/subscription/:token`、账号 access/token 返回、原始/native/Clash/sing-box/Zero 导出、二维码、用户端节点凭据与模板预览等可等价恢复配置的出口，必须由服务端统一拒绝/投影。不能只是隐藏客户端入口。管理员仍有运维权限，不在终端用户保密边界内。

切为 protected-only 时，事务性撤销旧订阅 access token；**已发出的节点凭据不会因撤销下载链接而消失**。若要阻止旧配置继续使用，还需服务端轮换/撤销节点认证材料，并验证相关节点已应用。不能宣称旧导出文件可被客户端远程擦除。

用户登录后不能自行降低 delivery policy 或调用别的格式绕过保护。每个新 renderer/API 都必须经过同一策略判断。ZBoard 的账号、订阅、设备/会话权限才是授权依据，客户端版本字符串不是设备可信证明。

## 4. 面板发钥，插件解密

职责按用户要求固定：**ZBoard 持有配置加密密钥，鉴权后下发短期解密材料；插件在自己的沙箱执行域接收密钥、解密并注入；客户端宿主只持有账号认证信息和公开状态。** 不设置宿主 ConfigVault，不在客户端包中内置面板主密钥，不持久化配置解密私钥。

这里宿主包括账号/UI/持久化/业务编排层，沙箱执行域包括插件组件及其受限 SDK 执行器。移动端可能共处一个 OS 进程，能保证代码/API 边界，不意味着同进程调试无法观察秘密。运行解密时，插件内存必然短时持有面板下发的密钥；Zero 在代理连接期间持有运行所需的明文状态。

### 4.1 密钥种类和所有者

| 材料 | 管理方 | 插件/宿主可见性 |
| --- | --- | --- |
| 面板主密钥、签名私钥 | ZBoard 服务端秘密存储 | 不下发；与 JWT/节点存储加密 key 分开 |
| 配置内容密钥 CEK | ZBoard 按账号/会话/配置响应随机生成 | 仅下发给已授权插件敏感组件；宿主不接收 |
| 用户登录/续期凭据 | 宿主账号服务与系统凭据库 | 插件用句柄；不取得通用登录 token |
| 最小权限 loader session | 宿主授权后交给插件执行域 SDK | 只用于绑定 origin/账号/订阅/端点，短期有效 |
| 配置密文、信封、key grant | ZBoard → 插件执行域的 TLS | 网络正文不经过宿主账号/HTTP/debug 服务 |
| 签名公钥、公开租约、版本、节点展示投影 | 宿主和插件可验证 | 不含 CEK、凭据和原始配置 |

配置密钥可以由面板长期主密钥保护，但主密钥永远不下发。不同租约/响应生成独立 CEK，避免一个静态 key 解开所有用户/历史配置。

### 4.2 交互顺序

1. 用户在宿主绑定账号并选择订阅。宿主取得最小权限 loader session，建立组件 generation、账号 scope 与运行实例绑定。
2. 插件敏感组件通过执行域内 SDK 请求密文。ZBoard 校验权益与 protected-only 策略，在内存渲染配置，以随机 CEK/nonce 加密，返回签名信封。
3. 插件向拟新增 `POST /api/v1/client/protected/key-grants` 请求本次信封对应的解密授权，携带短期 session、信封 ID/摘要、新鲜 challenge 和绑定信息。
4. ZBoard 再次验证账号/会话/订阅/配置版本/有效期与撤销状态，通过 TLS 返回 CEK 和签名 grant。SDK 在插件执行域接收，绝不先回传宿主再发给插件。
5. 插件校验信封/grant、解密，在同一执行域调用 `runtime.protected.load`。运行时入口返回固定回执；宿主只得阶段、版本、运行实例和结果。
6. 注入完成或失败后清零 CEK、解密字节及敏感临时对象。后续刷新重新向 ZBoard 请求授权，不从宿主/本地文件寻找旧 key。

会话建立不把 User-Agent/插件版本等同于可信设备证明。掌控客户端机器的人可能改写客户端或提取已授予的 CEK；该方案不承诺抵抗这种攻击。面板限时/撤销授权主要限制后续发钥，不会使已经读取的密钥在密码学上自动失效。

### 4.3 密文与授权绑定

初选标准 AES-256-GCM 加密内容，Ed25519 签名来源；使用维护中的密码库与双端固定测试向量，不实现密码原语。CEK 与 nonce 用 CSPRNG，每个响应新 CEK，严禁同 key/nonce 重用。[NIST GCM 标准](https://csrc.nist.gov/pubs/sp/800/38/d/final)。

信封包括版本/算法编号、面板 origin、账号/订阅/会话、envelopeId、keyId、configVersion、issuedAt/expiresAt、格式、nonce、ciphertext。key grant 包含 CEK、信封摘要、同一作用域、请求 challenge 和到期时间；CEK 所在响应不缓存、不写访问/异常正文日志。端点不能通过 bearer URL 暴露 key。

采用精确定义的字节编码：protected header 为原始 UTF-8 JSON 的 base64url；AEAD AAD 为解码后的 header 字节；签名覆盖域前缀及长度前缀编码的 header/nonce/ciphertext。严格 schema 拒绝重复 JSON key、未知算法/版本、越界大小；不反序列化再重排签名数据。

grant 签名覆盖 CEK 与对应信封摘要/会话/challenge，只有执行域验证其秘密部分。公开租约/展示信息另行签名，不能为证明租约让宿主读取整个 key grant。

签名 key 的信任由宿主独立注册：官方部署预置可信根，自托管由管理员提供指纹/首次绑定明确登记。插件包签名与面板配置签名独立，不能让同一未验证响应自行更新信任根。

版本由 ZBoard 按订阅单调增长；客户端宿主可保存最高接受版本等非秘密元数据。重放、跨账号/面板/会话、过期授权、错误信封摘要、旧 challenge 均拒绝。同版本续期必须保持配置内容一致；业务回退由面板生成更高配置版本，不降低防回放水位。

## 5. 通用能力如何承载此插件

ZBoard 包包含两个独立组件：普通面板组件负责套餐/用量/声明式页面；`protected-executor` 组件负责配置/发钥 API、内存解密及运行态注入。两者不共享 VM、KV、日志或任意消息通道。

| 组件 | 授权能力 | 输出 |
| --- | --- | --- |
| 面板展示 | `accounts.metadata.read`、受限公开 HTTP、`ui.contribute` | 公开 DTO，不能索取配置/密钥响应 |
| 受保护加载 | `credentials.use`、绑定的配置/key-grant 网络模板、`secrets.session.receive`、`crypto.session.use`、`runtime.protected.load` | 固定阶段/错误码/操作 ID/版本/公开签名租约 |

敏感组件允许在隔离域内处理 CEK 和配置。没有文件/剪贴板写、自由 UI/日志、任意 egress 或持久明文 KV 能力。常规 JS 字符串/GC 不提供可靠清零；SDK 使用独立 SensitiveBuffer/SecretHandle 及受限加密接口，插件控制解密/注入流程。需要解析正文的模块仍属于高信任代码，必须审计副本及错误路径。

沙箱并不能证明有明文访问权的恶意插件不会编码泄密。可信发布者、审查和禁止自由输出是该角色的要求；普通第三方扩展不能仅自行声明权限就取得此角色。密码原语可由同一执行域 SDK 提供，不能因此把解密搬回宿主。

`runtime.protected.load` 是通用语义能力，输入为受保护缓冲区句柄、运行目标句柄、期望 revision 与操作 ID；它没有读取配置、任意 invoke 或文件导出的对偶接口。执行器必须直接传到运行容器，不能序列化到通用宿主消息总线。

桌面适配为插件 worker 到 Core 的私有敏感通道。移动端由宿主 VPN/网络容器适配同一能力，插件不依赖 PID/socket/文件路径；若无法满足最低隔离与不经宿主正文要求，该能力为 unavailable，其他通用插件仍可工作。

## 6. 受保护来源与运行事务

普通本地配置继续原模型。新增 `ProfileSource = Local | ProtectedPanel`；后者仅包含账号/订阅绑定、密文摘要、版本、公开租约和展示投影，宿主不保留配置正文。平台中可有密文缓存，但首次版本默认不支持离线取钥/解锁。

1. 宿主确认目标运行容器提供受保护加载、无持久化及限制观测能力；缺失时拒绝该连接，不降级普通导入。
2. 桌面启动自己拥有的 Core，以无节点/无凭据 bootstrap 待命。配置日志/控制策略在启动时固定；秘密通道使用独立继承句柄，不能复用已承担 parent-lifetime 的 stdin，也不通过 argv/env/文件传 key 或配置。
3. 插件取得密文/CEK并在自身域内解密，提交类型化、大小受限配置。Core/执行器校验宿主锁定的控制面、日志、外部文件引用、事件 sinks 等政策，插件不能用配置修改这些边界。
4. 解密期间不占宿主配置锁；准备提交时宿主授予本次目标/revision 的短期操作租约，执行器/Core 再核对 scope、generation、configVersion 与有效期。完成前禁止与其他配置写事务并行。
5. 直接调用 Core runtime-only 配置事务，复用 listener/service reconcile 和 last-known-good 内存回退。现有 GUI `config_apply` 的身份确认语义需要抽成无明文、可被执行器复用的契约；不能直接调用现有会捕获正文的协议服务。
6. 核对同一运行实例的应用 revision 后才返回成功。受保护节点/策略快照由 Core 允许字段投影，宿主保留安全回执而非原始配置。
7. 系统代理/TUN/VPN 开关仍由宿主连接事务管理并读回状态，配置成功不等于外网已验证。插件不能接管手动断开/恢复。

Core 增加通用受保护实例政策，禁止后续普通 config.apply、模式/DNS/规则操作间接写出秘密，也禁止改变观测/日志策略。一次 `persist=false` 不等于整个生命周期无明文持久化。Core 不解析面板登录、密钥或套餐。

## 7. 所有明文出口

| 出口 | 强制控制 |
| --- | --- |
| 宿主 profile get/list/import/export、编辑/复制/二维码 | 受保护来源只返回公开 view，后端拒绝正文读取和来源降级 |
| SQLite/WAL、JSON/临时/启动文件、升级备份 | 无受保护正文和 CEK；升级/校验/恢复不能生成临时明文 |
| 插件 HTTP、IPC debug ring/JSONL、日志/错误 | 敏感响应只在执行域；捕获前按固定类型投影，不先 clone 明文 |
| Zero Config/Status/Runtime/Policies/events/diagnostics | 保留别名、能力、计数、延迟，隐藏端点/凭据/敏感规则与错误中的地址 |
| 原始 IPC/HTTP/gRPC 和诊断 endpoint 覆盖 | 受保护实例限制控制入口和调用者，不能同 UID 自动获权 |
| 组件输出、文件/剪贴板/UI/通知 | 敏感角色仅固定回执，不共享普通组件自由输出能力 |
| ZBoard 旧链接、格式/模板/账号 access/节点接口 | protected-only 在服务端统一限制，关闭等价明文旁路 |

日志用字段允许列表，不用秘密字段名黑名单。密码缓冲区清零、限制 core dump/锁页能减少暴露，但锁页不涵盖所有栈/寄存器与 OS 转储；普通 JS GC 和一次清零无法证明所有副本消失。[安全内存限制](https://doc.libsodium.org/memory_management)。

## 8. 续期、暂停与故障

采用在线短租约，初始建议 5 分钟/提前 60 秒续期，实际由服务端与移动网络验证确定。签名有效期结合单调时钟；休眠/恢复时立刻核验，不因墙钟回拨无限延长。

- 拉取/取钥/解密失败：仍在有效租约内的旧配置继续运行，显示安全错误；不降级明文、不重启内核。
- 应用结果 uncertain：冻结该写事务并查询运行回执，不能重放或宣称已回滚；手动断开和到期清理仍可执行。
- 断网/后台暂停：不依赖插件定时器维持网络安全；宿主/运行容器执行租约截止，插件恢复后重新取钥。到期停止本次保护连接并清理本次托管系统设置，不悄悄改变用户显式 fail-closed 策略。
- 注销/停用/卸载：先递增授权 epoch，使旧句柄无效，再走宿主断开，清理插件秘密与运行态。不能因插件清理回调卡住阻塞退出。
- worker 异常：在租约内不自动断开正常连接；重复崩溃隔离插件。主动停用与崩溃的策略不同。
- Core/移动运行容器崩溃或升级：恢复到无秘密待命状态，在线重新取配置/密钥；不能使用旧 export 文件恢复。
- 配置失败回退仅允许本事务内仍合法的旧内存配置；插件包回退不恢复旧 token/租约或降低配置版本。

服务端撤销限制后续发钥，在线收到拒绝立即清理；离线最多到当前租约截止。已经被提取的密钥/凭据不能靠客户端租约销毁，节点侧权益和认证撤销仍需服务端执行。

## 9. 交付范围与验收

本方案是通用插件能力的首个组合案例，不向框架加入 `ZBoardManager` 或面板专用授权捷径。客户端需账号句柄、组件沙箱、敏感 IO/运行态能力与公开视图；ZBoard需受限会话、protected-only、发钥/密文端点和撤销；Core/移动容器需受保护运行与观测政策。

1. 用 canary 配置走登录、连接、刷新、升级、失败、暂停恢复和退出：宿主消息/堆采样不出现 CEK/正文；应用目录、SQLite/WAL、临时文件、日志和诊断导出无明文。该证据不等于管理员/OS 内存绝对保密。
2. 直接调用各宿主/运行时查询、原始控制入口及模式/日志/配置写操作，不能恢复正文或创建明文文件。
3. ZBoard旧 token、所有导出格式/模板/access/用户节点入口执行 protected-only 测试；standard 行为保留；切换政策后的节点凭据撤销有实际结果。
4. 双端加密向量与篡改/跨账号/跨会话/错误摘要/重放/过期/key 轮换测试通过。检查 CEK仅由面板签发，宿主没有内置/持久配置 key。
5. 越权插件/同包其他组件无法调用敏感 HTTP、读取敏感缓冲区或伪造运行目标；停用后的晚到结果无效。
6. listener失败、响应丢失、并发编辑、worker/容器崩溃、租约截止、移动后台/休眠恢复不破坏用户手动断开与 last-known-good/uncertain 语义。
7. 桌面安装包与移动原型跑同一能力契约测试；明确每个平台的隔离强度和未支持能力。客户端/Core/ZBoard按各仓库要求完成相关测试和端到端联调，不能用加解密单元测试替代整个闭环。
