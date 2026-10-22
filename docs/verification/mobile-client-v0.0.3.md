# v0.0.3 移动客户端适配验证记录

## 2026-10-20：实际 Keychain 拒绝与构建 App 元数据取证

`6fd3dc42b5784bcfccc1d369b5ba261752bff6ea` 修正后，云端 PR Simulator
job **118410447966** 的实际回执为 protected-data=true；生产 probe、隔离 probe、
创建前读取均 OSStatus=-34018。停止在 readBeforeCreate，创建/回读/清理未执行，
roundTrip=false，SDK unavailable/accepted:false。保留原始日志，不能称为 Keychain
正向验收，也不能由此推出必须购买开发者会员。08d 首次编译失败继续单独保留。

为按 Apple DTS 建议核对实际构建物，完整 App 验证器增加只读 entitlement 摘要，
不改签名或生产准入。摘要只含固定状态、存在/匹配布尔值及数量，拒绝畸形与过大 plist；
检查失败不会假装空 entitlement。Python **14通过**（新增1项边界用例），Node宿主
组合 **11通过**。本地无 Apple SDK，真实构建物摘要待本提交云端 CI，不能由 fixture
推断实际 App 内容。原生文件票据仍在独立工作区开发，未随此批开放 SDK。

## 2026-10-20：iOS Debug 隔离 Keychain 分阶段诊断

基于 `24c5a2f33645309ce0697ebe758483961beddd05`，同一 Draft PR #46。
[诊断契约](../gui/mobile-ios-keychain-diagnostics-v0.0.3.md)区分最近生产 protected-data
gate、原 metadata probe 是否执行及 OSStatus，与独立临时条目的 probe、创建前读取、
随机材料、创建、回读、清理。Debug 环境 nonce 精确匹配才可执行，不读写正常 master
或用户数据，不生成 Apple 签名凭据或配置 entitlement。原生产 query 与拒绝条件保持。

独立探测可在生产 gate 拦截时访问自己的临时条目，让系统给出实际状态；不解除系统
保护或 SDK 准入。隔离匹配回读与清理成功也不自动改成 SDK accepted:true。OSStatus
只以 Int32 传递，未执行保持缺失/null；没有 key、account、URL、令牌或异常描述字段。
Rust 严格校验阶段，JS 显式投影，Python 验证公开回执；后续 UI 失败仍可保留安全诊断。
原生桥缺回执不自动重复。已有不可用日志不能证明根因是账号、签名或某一系统错误。

最终 locked workspace/all-targets/all-features Rust **1627通过/0失败/6忽略，40组**，
GUI **1343/0/5**，native-io策略及诊断 **16/0/0**（新增3项诊断约束）。fmt、workspace
check/Clippy、NDKr27d/API24完整宿主通过，既有告警保留。Node driver **6/0**、Python
宿主验证 **13/0**、iOS宿主Node组合 **11/0**，类型 **0错误/0警告**；最终回归包括隔离
正向事实与生产安全拒绝并存、未执行却伪造status/成功、额外秘密字段及后续失败的安全投影。
早期与最终日志分别保留。本批未修改应用Svelte/TS页面，不借用旧浏览器运行冒充新测试。

本地没有 Swift/UIKit/Security SDK；新增 Swift 模型测试、实际原生 gate/OSStatus/创建/
回读/清理和完整 App 必须由本提交云端 CI 核对，不能预先声称 Keychain 正向存取通过。
真实 iPhone、Galaxy S8、锁屏/重启、文件/浏览器交接及系统 VPN/内核装配仍未验收。
证据：`/workspace/artifacts/mobile-v0.0.3/ios-vault-diagnostics-24c5a2f/`。

08d979a 首轮 iOS device job118406983248在实际iOS Rust编译时报E0382：新增诊断提前
消费了Option<AppHandle>，后续原有正向检查再次使用。改为诊断只借用句柄；保留首轮日志，
修正提交的真实SDK/App结果继续单独核对。此失败尚未运行Keychain探测，不能归因账号、
签名或某个OSStatus。同期Swift策略与iOS IO适配器SDK构建通过，也不替代完整App结果。

前一 `24c5a2f` 的12工作流最终均成功；PR主CI首轮Windows job118365411856在45分钟
边界取消，Release **65通过/1忽略**，后续readiness未完成。只重跑该job，attempt2
118382652443同SHA成功，Release仍65/1，实际released-kernel readiness **2通过**。
首次取消与重跑日志分别保留，没有修改超时或断言。其四侧Chromium/WebKit各479通过、
无浏览器重试；两侧iOS实际VM/同PID生命周期成功，Keychain均unavailable/accepted:false。
这些是旧头实际结果，不是本次新增诊断已经在真实 Keychain 运行的证据。

## 2026-10-19：原生插件可见文件策略基础

在回调原授权加固及返回键修复后，原生IO新增独立的插件可见文件用途，详见
[原生文档边界](../gui/native-documents-v0.0.3.md)。Rust/Android/iOS共同限制4MiB，
仅系统读取或明确保存；保存须确认凭据风险，不允许系统分享。Rust另检查字节所有权，
受保护材料对所有导出用途继续拒绝；二进制MIME不扩大设置或普通配置的契约。
Rust在占用原生操作锁及调用平台前核对动作与用途。

最终 locked workspace/all-targets/all-features Rust **1624通过/0失败/6忽略，40组**；
GUI仍为 **1343/0/5**，共享原生IO策略13项通过。真实Android库/JVM/测试APK构建成功，
JUnit **15通过/0失败/0跳过**（策略14项，安全存储envelope1项）；不是在模拟器执行测试APK。
locked workspace/all-targets/all-features check/Clippy、NDKr27d/API24完整Rust宿主、
格式检查通过；本地没有Swift工具链或Apple SDK，本批Swift
新增测试和iOS编译须由新头CI核对，不沿用旧头成功结果。既有告警保留。

本批没有新增WebView invoke或开放文件SDK。宿主批准、分块传输、原审阅/安装/授权绑定、
预期系统暂停后的新页面消费及未知写结果屏障仍待实现；移动文件读写继续明确不可用。
没有系统picker、Galaxy S8、iPhone或真实插件文件交接验收。
证据：`/workspace/artifacts/mobile-v0.0.3/mobile-file-policy-c56b267/`。

## 2026-10-19：移动返回键只交付最上层弹层

`450dd95` 的 push 主 CI 成功，但 PR 主 CI 的 WebKit job `118345636704`
在规则迁移的 Escape 用例首轮和 retry #1 均失败；聚合 job 随之失败。此记录不以其他
成功工作流覆盖。排查发现移动壳层未识别不带 `data-state` 的自定义 dialog，又将已收到的
Escape 再合成为 Escape，干扰嵌套菜单自己的键盘处理。

新增真实键盘和系统返回两个事件计数反例，修复前 **0通过/2失败**，保留截图、trace和
原始日志。修复后，键盘事件直接留给所属菜单/dialog；系统返回只向当前可见菜单优先
交付一次 Escape，已关闭但仍在退出动画中的 portal 不获得下一次返回。
原有 Escape 用例未放宽，导航与规则迁移 Chromium **20通过/0失败**；随后 DNS 迁移、
设置草稿、配置编辑、恢复及 VPN 设置弹层另 **70通过/0失败**，均无重试或跳过。
完整 `verify:frontend`、类型0错误/0警告及生产构建通过。

本地标准 Playwright WebKit 安装的各官方下载端点均返回网络策略 **403 Domain forbidden**；
没有绕过策略、增加测试超时或换用用户 Mac。后续提交的真实 WebKit 结果须以其 CI 为准。
证据：`/workspace/artifacts/mobile-v0.0.3/mobile-menu-escape-450dd95/`。

## 2026-10-19：原生交接前的共享回调原作用域与预算

基于已推送 `450dd950a47c0559248a7b3809d5ac1f4cdfad45` 继续。实际TCP+SDK反例先得到
**0通过/1失败**：持久授权撤销后恢复，新调用仍能访问原回调。保留失败日志，修复后原反例
通过。会话现固定原审阅/安装generation/宿主代次；创建、取消取得锁后及发布前复查原lease，
读取交付前核对原对象，不靠同名组件重新授予旧资源。原始store方法收窄至私有，只经宿主
原作用域 helper 接入SDK；不暴露指纹/租约。详见[SDK回调契约](../gui/plugin-sdk.md)。

新增32总量/8单组件预算，过期与失效原归属安全清理，线程创建失败保留固定安全原因。
5分钟单调时钟控制会话，超时清除临时结果。HTTP头8KiB/总读取2秒，持续碎片不延时；
重复字段、无效编码/UTF-8、错路径/请求行及超限拒绝，不覆盖state。接收网页只确认传输
收到并要求回应用核对，不宣称业务认证成功。没有扩大callback scope或改变既有成功DTO。

最终 locked workspace/all-targets/all-features Rust **1621通过/0失败/6忽略，40组**，
GUI **1343/0/5**，15项回调测试通过（原2项+新增13项）；先修复作用域及编码的5项、中间
预算12项均通过，最后复核包含真实同包卸载重装、旧授权恢复、发布/锁等待期间撤权、
有界线程/结果、绝对头部期限、单调到期、编码及固定错误脱敏。所有TCP为云端隔离测试，
不访问用户配置、账号或真实手机。新SDK错误只发固定原因；不将回调token/正文写入日志。

最终fmt/Clippy、NDKr27d/API24完整host及类型0错误/0警告通过，既有告警保留。首轮新增
private-interface警告通过收窄store方法消除，不关闭lint；原0/1失败和全部重跑记录保留。
本批未改UI/TypeScript SDK或依赖；450批69项浏览器不是本批重新执行，没有声称本地WebKit
或Apple SDK运行。源码/测试不代替原生文件、浏览器返回或真机交接。

证据：`/workspace/artifacts/mobile-v0.0.3/mobile-callback-450dd95/`。移动BrowserCallback
及文件读写仍由平台明确阻止；原生方案需处理授权保持、系统暂停、独立新页面核对和
结果消费，不能把共享loopback实现当作手机适配。A21及后台/真实手机继续未完成。

450头CI观察：push完整iOS device/Simulator两job成功，实际VM与同PID24906的
ready/suspended/ready、generation1/2/3、executor true/false/true通过；Keychain仍
unavailable/accepted:false。PR两iOS job也成功；其原始console继续核对，不借作当前
回调新头证据。其余450工作流仍按实际结果更新，不提前称整个CI或任务执行验收完成。

## 2026-10-19：原生前台插件任务与持久结果屏障

在普通存储提交 `40416941f405bc2692d8839a354334f669c89e41` 后继续，同一 Draft PR #46。
[前台任务 owner](../gui/mobile-plugin-tasks-v0.0.3.md)复用共享队列与原权限/预算，原生 profile
核对后唤醒单一 Rust worker；后台封闭原 executor、停止旧代次剩余排队任务，真正 Exit
释放 worker 的 AppHandle 引用。ExitRequested 保持可恢复暂停，不更改 VPN 生命周期。
平台与320px页面明确仅前台、OS后台未适配；iOS Release 外部插件限制继续保留。

每任务一个有界 lastAttempt，固定原审阅/安装generation/意图修订/attemptId。队列记录
先落盘，跨 VM 前保存 dispatch 准备标记；标记不证明 VM 已执行成功。旧 queued 可确认
取消，旧 running 失去 owner、授权或代次后为 unconfirmed；后者禁止宿主自动新周期或
换号重放。正常插件结果的显式 ok:false 才沿用后续退避；原执行错误保留未知。明确新保存
意图不等于确认旧副作用。新增 readonly Rust/TypeScript DTO，新的时间为字符串、缺失
完成时间为null；旧无历史记录只表示未提供。继续256项/256KiB预算，不承诺无限历史。

共享调度的发布校验进一步移动到临时文件刷盘后、persist之前；CBC批原检查在进入 writer
之前，本次覆盖等待 IO 阶段的撤权。测试要求真实 staged tempfile 已存在才撤销原 lease。
VM状态更新仍经原namespace发布边界；重装、同值替换、删除、旧授权不能改变新任务。

最终 locked workspace/all-targets/all-features Rust **1608通过/0失败/6忽略，40组**，
GUI **1330/0/5**；新增7项 journal、1项原生代次模型及原8项调度回归通过。此前两轮1608及
退出清理候选1608均保留，最终再复核生命周期和lint作用域修正后的代码。最终Clippy/格式检查通过，既有告警保留；真实 NDKr27d/API24 完整host最终通过。完整前端验证、类型
0错误/0警告、生产build通过；最终四份插件Chromium共 **69通过/0失败/0重试/0跳过**，
含320px浅深色20px根字号的任务限制及原安装/页面/授权/卸载流程。首次11项移动host也
通过。新增iOS宿主Node复核9通过，不称Apple SDK或真机任务运行。未运行本地WebKit。

开发期失败保留：初次模块Owner可见性导致Rust re-export编译错误，按父模块边界修正；
首次API24因原生暂停访问私有wake字段编译失败，增加窄notify方法，不暴露整个内部状态。
后续最终NDK复核通过。Clippy发现新增的无效drop(State)，改成词法作用域后重新测试，
不关闭lint。旧候选结果不代替修正后结果，没有跳过Husky或增大测试重试/超时。

证据：`/workspace/artifacts/mobile-v0.0.3/mobile-task-owner-4041694/`。原生代次/进程owner
重建为Rust模型测试，浏览器fixture不是真手机任务执行；实际OS后台、Galaxy S8、iPhone、
锁屏/低内存、原生插件文件/浏览器handoff及受保护运行仍未验收/适配。不能称A19/A21或
整个移动端完成。新头的Apple构建与CI必须另行观察。

前一头 `cbc7909ebd1b1cc3a41532ffcb78c282e8a60c15` 的push/PR共12工作流全部成功，
**56成功jobs、2明确跳过的iOS embedded-owner jobs**，Android embedded-owner步骤仍为
workflow明确关闭。两侧Chromium/WebKit各475通过，0失败/retry/flaky汇总。两侧API24
emulator各实际执行1项Keystore原语通过，不替代Android Rust/JNI/guest全链或Galaxy S8。

CBC两侧iOS device/Simulator完整App共4jobs成功。实际Simulator VM成功，同PID生命周期
ready/suspended/ready、generation1/2/3及executor true/false/true：push PID34174，PR PID37251。
Keychain仍unavailable/accepted:false，仅计安全拒绝。App floor16.4；每包469项许可输入、
12明确缺口。PR实际编译合并提交f98b53d1be3a3e571a93b64629a8d91067b48755，父为d39+cbc，
其tree753bcc8669f4fc60fc385dfe97c9ee3c3ccd3b7e与CBC head相同，已通过Git API核对。
原始console及12工作流最终job清单在`mobile-namespace-cbc7909/`，不冒称本地读到artifact ZIP。
这些是CBC历史证据，不是当前新代码已通过CI；也不证明此前FAB无回执失败的根因已修复。
FAB push主CI39327636400实际因后续推送取消，原Simulator失败记录继续保留。

## 2026-10-19：插件普通存储原授权发布边界

基于 `cbc7909` 继续同一 Draft PR #46。普通 SDK 新增、删除、清空和迁移在等待管理锁后，
及临时文件刷盘、正式发布前核对原 lease；旧调用不借用后来授权。VM 运行状态更新在
发布前核对原执行回执、审阅与宿主代次，调度更新继续核对原意图。宿主可信桌面入口保留，
移动原始命名空间命令的拒绝边界不变。详见[SDK 普通存储契约](../gui/plugin-sdk.md)。

迁移仍为内容/版本标记两个发布步骤。第二步失败给出固定脱敏 `uncertain`，明确先读取
真实内容与版本，不自动重放、不声称回滚。原授权已失效时仍拒绝旧结果交付；不会为了
传递错误重新授权。单文件发布增加标记目录同步。测试使用真实沙箱授权 lease 和隔离
文件系统，覆盖 state/cache 四种操作锁等待撤权、发布前撤权、两步之间撤权、标记真实
IO 失败、晚到运行态更新及正常分区/逐版本行为；不碰用户配置或真实隧道。

最终 locked workspace/all-targets/all-features Rust **1600通过/0失败/6忽略，40组**，
GUI **1322/0/5**，7项 namespace 回归全部通过。首轮测试编译因误写权限枚举名称失败；
修正后针对性6项通过，补标记 IO 反例后重新执行完整1600项，不用中间结果替代。最终
真实 NDKr27d/API24 完整 Rust host check 通过；首次命令误传不存在的 `mobile` feature
被 Cargo 在编译前拒绝，按仓库实际默认功能命令复核并保留原日志。类型0错误/0警告。
最终Clippy与格式检查通过，既有告警保留，不用旧提交结果替代。未改前端 UI，不重复计入旧
浏览器结果；实际 Apple 构建及前后台验收仍观察本提交 CI。

证据：`/workspace/artifacts/mobile-v0.0.3/mobile-namespace-cbc7909/`。本项不开放尚未适配
的移动原生任务 owner、系统后台、插件文件选择、浏览器回调或受保护运行通道，也不是
整个 A20–A22 或手机验收。Android API24/Galaxy S8、iOS16.4 目标和真实手机/VPN缺口保留。

## 2026-10-19：共享插件任务原作用域与发布校验

基于 `fab55c0` 继续，先独立保留无回执启动诊断提交 `81d3dc8`；本批不开放尚未适配的
移动任务权限，也不启动 UI 定时器冒充平台调度。每次保存产生新的不透明意图修订；排队
固定原组件审阅、安装generation与原生代次。执行前、运行状态更新前及完成回写校验原
对象，不借用重新授权或重装后的对象。同值重存、删除和旧完成不修改新任务的计数/时间，
也不推送替代任务的失败提示；已发出的外部请求仍可能完成，不声称强制撤回。

调度新增/删除在取得原管理锁后与发布前核对原lease。补齐既有256项数量预算的写入校验，
避免写出随后无法读取的257项文件；字节预算同样在发布前拒绝，原文件保留。满额可更新
既有意图，旧无revision记录保持可读，不能由此宣称旧客户端降级兼容。测试拆入同模块
文件，不新增前端业务队列。细节见[SDK调度边界](../gui/plugin-sdk.md)。

最终locked workspace/all-targets/all-features Rust：1594通过/0失败/6忽略（GUI1316/0/5），
包含8项调度回归；此前无数量检查的作用域全套1593通过也保留。真实授权lease在等待锁后
撤销、发布前撤销、真实store卸载重装、旧授权重授和同值ABA均验证文件不变。
fmt/Clippy通过，数量边界小变更后的最终NDKr27d/API24完整host复核也通过；
类型0错误/0警告、owner回归3项通过。未改前端UI，上一安装批67项Chromium不算本提交重新
执行。Swift启动诊断在本地仅Node9/Python12，实际Apple检查仍待新头CI，不借用旧SHA。

证据：`/workspace/artifacts/mobile-v0.0.3/mobile-scheduler-fab55c0/`。原生前台任务owner、
OS后台执行/恢复、文件票据、浏览器回调与受保护运行通道仍逐项继续；无手机调度或VPN
验收结论，没有用户Mac、发行凭据、合并或发布操作。

## 2026-10-19：iOS无回执启动诊断

安装回执提交 `fab55c087b4a2b0e73de2fe57d6b97904791f795`：PR完整iOS的device与
Simulator通过；PR真实console记录同PID35247、ready/suspended/ready代次1/2/3及
executor true/false/true（[实际job](https://github.com/zerodenet/znet-sink/actions/runs/39327645924/job/118317500636)）。
VM成功，Keychain仍 unavailable/accepted:false。push device成功，但Simulator完整包
构建后未收到UI/IPC回执，console为空，原失败与截图artifact保留（[失败job](https://github.com/zerodenet/znet-sink/actions/runs/39327636106/job/118317467369)）。
此时原因未知，不把另一侧成功当作push通过，也不推断一定是测试环境。

后续只新增显式Debug nonce下的原生初始化、WebView绑定、等待文档、派发驱动及回调阶段
证据；进度文件与最终收据分开，不能通过UI/IPC验收。只记录固定枚举、PID及协议/加载
布尔，不包含导航地址、异常正文或配置；缺失、损坏、旧nonce和过大进度不回显原文。
超时时保留启动器存活/退出码及console大小，成功断言、等待预算和无凭据约束不变。
没有自动重试或放宽验收。Node驱动9项、Python验证器12项通过；新增进度拒绝最终收据的
首轮测试误限定异常类型（实际KeyError），修正拒绝断言后重跑。Swift实际SDK仍待此提交CI，
不将诊断源码当作根因修复或手机验收。

证据：`/workspace/artifacts/mobile-v0.0.3/mobile-install-receipts-1acab71/ios-118317467369.log`
及同目录成功日志。共享调度安全改动仍是另一批未提交工作，不混入此诊断的旧SHA结论。

## 2026-10-19：原生包读取回执与安装材料核对

在 `1acab717b3cbd805b6e9862b4f1728631730133d` 上继续。取消不创建计划，读取失败按
容量、权限、文件提供者区分；缺回执/非法回执保持结果未确认，原生原因和路径不出桥。
页面提交后原计划只能用于释放待审阅资源，不能再次确认；关闭/离开也释放仍未提交到宿主
的计划，不声称撤回已执行安装。迟到的旧页面回执不触发新安装或成功提示。

新增主页面专用只读核对命令：冻结原插件ID、候选版本、摘要和发布者指纹，重读本地登记
和实际签名包，区分材料一致、不同、未安装和无法核对。不联网、不替换、不改授权/启用，
也不以组件列表缓存代替包事实。返回时间使用字符串，页面显示核对结果并保持确认不可重放。
材料一致不等于组件已运行；无登记、签名失败或损坏保留未确认。状态与触控布局见
[宿主契约](../gui/mobile-plugin-foreground-v0.0.3.md)。

当前本地：7项原生包/回执回归通过，完整 locked Rust workspace/all-targets/all-features
1588通过/0失败/6忽略（GUI1310/0/5），Clippy及API24完整host通过、告警保留。前端完整
验证通过，类型0错误/0警告及生产构建成功。新增 UI 首轮5通过/2失败，均为 Toast与面板
错误的双重alert定位，限定dialog后重跑；记录保留，不称首轮全绿。中间插件全套65项及
大字号布局全套67项通过，最终计划释放与迟到回执保护后再次核对67项通过，最终类型0错误/0警告及生产构建通过。浏览器fixture不是真机系统选择或安装验收。

证据：`/workspace/artifacts/mobile-v0.0.3/mobile-install-receipts-1acab71/`。本批未改签名
发行、VPN/TUN或core运行；前一安全存储SHA实际API24原语与Apple结果见下段，不能借用
为当前提交CI或完整手机验收。

## 2026-10-19：原生插件安全存储（继续实施）

在 `497250e28084e1dbc476d62e1e509bfe5e129421` 上继续，同一 Draft PR #46；
新增[原生安全存储契约](../gui/mobile-plugin-secure-storage-v0.0.3.md)。AndroidKeyStore
包装主密钥、iOS ThisDeviceOnly Keychain 与私有 native-io 桥接入原 SDK；真实 API 准入
不等于操作成功。移动不编译桌面明文主密钥后端，既有身份丢失/变化、损坏和旧格式拒绝
替换或降级。原 lease 在原生 IO 前后重查；UI 重新检查不创造原生前台权限。

本地已执行：完整 locked workspace/all-targets/all-features Rust 第一轮1583通过/0失败/
6忽略（GUI1305/0/5），最后新增固定安全错误分类后插件回归109通过/0失败；随后将新建设备
种子在失败路径也置于 Zeroizing，最终全套1584通过/0失败/6忽略（GUI1306/0/5）。
最终Clippy、fmt与API24 host再次通过，现有告警保留。真实 NDK
r27d/API24 完整 Rust host check通过，当前 Cargo resolved 图417项/3明确许可缺口；
不宣称最终逐包链接或法律合规。现有告警保留。

安全存储提交 `1acab717b3cbd805b6e9862b4f1728631730133d` 的 Native documents
push/PR各4 jobs成功；真实 Android 7.0/API24 emulator instrumentation各执行1项，
覆盖原生包装、恢复、不可导出、损坏和丢失密钥拒绝替换（[push实际job](https://github.com/zerodenet/znet-sink/actions/runs/39324355239/job/118306216974)、
[PR实际job](https://github.com/zerodenet/znet-sink/actions/runs/39324361966/job/118306240250)）。
这是原生存储原语的实际执行，尚不证明 Android Rust/JNI/guest 全链或Galaxy S8。Swift策略
和 unsigned iOS IO库SDK也已在同SHA通过。随后push/PR完整App的device与Simulator各job
全部成功（[push Simulator](https://github.com/zerodenet/znet-sink/actions/runs/39324355720/job/118306439519)、
[PR Simulator](https://github.com/zerodenet/znet-sink/actions/runs/39324361911/job/118306643404)）。
push产物commit为1acab71；PR产物commit为GitHub测试合并a3d282f（parents d39ad27、
1acab71，tree与head相同），两份Simulator包各469项许可输入/12明确缺口，不称合规。
真实WKWebView/Rust回执 nativePluginVm:true；强制执行的同PID Settings切后台/回前台
验证通过。Keychain实际回执为 unavailable、accepted:false，只计SDK拒绝契约通过，
**不计Keychain可用**，不推测具体授权错误或创建凭据。Artifact转存URL在本环境返回403，
未声称读到本地ZIP内生命周期原文；保留GitHubjob、console回执与artifact摘要。后续仅输出
已验证的PID/phase/generation/executor布尔白名单，方便直接核对而不泄漏新增原生字段。

Android原生库、13项JVM测试及实际隔离测试APK构建通过（82个实际任务）。检查合并清单
确认测试包、Debug和min24，SDK临时测试签名与发行凭据分开。原生 AndroidKeyStore 的
独立CI运行记录见上段，不能把JVM或APK编译当作运行成功。首次构建与
Rust链接因本环境磁盘耗尽失败，保留日志；仅迁移本任务生成缓存到临时盘并保持原路径链接，
未重置源文件或删除证据，重新构建成功。

iOS新增严格请求/Debug nonce专用Keychain account与清理、真实AEAD及丢失密钥拒绝检查。
Linux未运行Swift、UIKit或Keychain。JS驱动9项、Python验证器10项通过；缺API时必须返回
unavailable/accepted:false并阻止三类SDK角色，该分支不算安全存储可用。实际Apple SDK、
Simulator VM/生命周期/安全存储收据及exact-head CI待提交后运行，不沿用旧SHA结果。

证据：`/workspace/artifacts/mobile-v0.0.3/mobile-vault-5a99c4a/`。Galaxy S8/iPhone、锁屏、
重启、低内存、备份/恢复、签名发行未验收；无用户Mac、账号或发行凭据操作。文件票据、
浏览器回调、系统调度和受保护运行加载仍逐项继续，不把它们整体归为core或已获后置。

## 2026-10-19：移动插件前台宿主（继续批次）

`5a99c4a` 实际 CI：Android Debug push/PR、unsigned Release push、Native documents
push/PR及Mobile Rust push已成功；iOS PR device SDK包成功，但Simulator实际UI/IPC回执
失败（`ipc-or-script-failed`）。完整App及QuickJS SDK构建已完成；失败发生在真实运行阶段，
不能记作移动VM或生命周期通过。原日志保留为本批 `ios-pr-simulator-failure.log`，实际
Simulator包468条许可输入/12明确缺口。后续诊断提交只增加固定阶段和白名单错误分类，
另加独立算术检查，不回传异常原文、用户数据或任意错误字段，不降低验收断言。正在推进的
平台安全存储另为未提交改动，不混入这次失败的既有提交结论。

诊断头 `5acb40f`：PR Simulator无回执失败单独保留；push Simulator给出真实
`ui-smoke-plugin-vm-ipc / native_vm_smoke_failed / arithmetic / IncompatibleDevice`
（[实际job](https://github.com/zerodenet/znet-sink/actions/runs/39317614933/job/118285466536)）。
原因是共享解释器的最终兼容校验仍重新构造 desktop Target，未使用宿主已核对的移动
设备。后续修复在原 owning operation 锁内固定并传入 Target，保留实际编译 OS/ABI、
manifest、版本、权限、预算和隔离校验；legacy desktop入口仍不得猜测手机形态。
增加 Any-target 声明也不能伪造宿主 ABI 的真实 VM 回归。修复后的 Apple执行结果待新头CI，
不把先前SDK编译完成算作VM已运行。

基线 `2b411b6c21c3ad3ef59a7589dc6e2ec85116e03f`；远端 develop 仍为
`d39ad2787fb05052fa15065853ab71526441ccc3`，已包含在本分支；仍是 Draft PR #46。
新增范围见[前台宿主契约](../gui/mobile-plugin-foreground-v0.0.3.md)。当前批 Apple
SDK/Simulator、打包结果和 exact-head CI 尚待实际运行，不沿用上一批12条绿色流程。

开发中已执行：真实 r27d/API24 完整 Android Rust 宿主 check 通过；首次完整 check
因旧 Release 清单被当前 Debug 拒绝，后按实际 target/profile/features 重新生成后通过。
后续发现两处日志分类仍被桌面 cfg 排除，修复后完整目标再次通过。以上失败保留在证据中。
Android 原生 IO 库实际构建和12项JVM测试通过；新增 Swift 策略测试待 Apple SDK CI。
Linux 完整 Rust 回归1572通过/0失败/6忽略（GUI1297/0/5），固定生产 executor VM
实际覆盖解释器、身份、未授予拒绝、超时及撤权；这不是手机 VM 运行证明。
原生恢复代次、一次性签名安装、摘要错误/过期/取消/替换拒绝、旧草稿/控制 review、旧文档
无法复活均有回归。最后小变更仍重跑相关/全量检查，以本批最终收据为准。

新增3项窄屏 Chromium UI 测试通过，覆盖原审阅草稿、原生包选择/固定凭据和未核对列表；
首轮两项失败分别为 fixture 对 Svelte Proxy 使用 structuredClone、产品空态条件分支不可达，
均修复后重跑，不能称首轮全绿。插件/签名管理页完整 Chromium 回归47通过；首轮46通过/
1失败，为新增未核对空态的安装入口重名，修正后47项重跑通过。完整移动端和 exact-head
WebKit结果继续记录。类型检查0错误/0警告、移动前端回归、生产构建、许可回归及Clippy已执行；
最后提交前继续核对变更后的结果，不把源代码断言或模拟UI当成原生验收。

QuickJS两个缺失正文通过实际 `.crate` 校验值及 `.cargo_vcs_info.json` 的固定提交
`f3039c8d931417049ce6133b3ff4550ca38438a5` 获取原始 LICENSE，缓存保留原文、源URL、
提交与摘要。Android实际 Rust图417项/3项明确缺口；另有前端声明及Apple实际输入/材料
缺口按最终产物分别列出，不据此宣称法律合规或最终逐包链接证明。

证据目录：`/workspace/artifacts/mobile-v0.0.3/mobile-plugins-2b411b6/`。真实 Galaxy S8、
iPhone、系统文件选择交互、通知呈现、低内存/锁屏、VPN/TUN/网络切换、签名发行与完整
SDK仍未验收；没有操作用户Mac、购买/创建签名凭据、合并或发布。

## 2026-10-19：develop 整合、主应用许可与本地容量

从 `9c03d26` 整合 develop `d39ad2787fb05052fa15065853ab71526441ccc3`，采用独立
工作分支的非快进合并；仍是同一 Draft PR #46，不推 develop。统一桌面配置目录、配置级
策略选择和 DNS 共享资产与既有移动两分段、无损文本编辑、固定删除预览和未知回执链共存。
DNS 资产在最终引用校验之前物化，失败仍拒绝不完整候选，不关闭引用完整性校验。

新增 iOS 固定许可 reader 与两轮标准 CLI 构建：首轮收集本次 target 的 Cargo/Vite/各
SwiftPM 真实输入，次轮通过资源流水线打包，最终核对包内字节、最新材料、SDK 与实际 IPC。
不在 App 产物上事后注入法律文件。库级清单、主应用 resolved-input 资产、Simulator
执行和 unsigned device-SDK 包仍分别记录；脚本存在或 Linux fixture 不算 Apple 验收。
四项已审计原文/声明缺口及最终逐包链接证明限制保持，见[许可说明](../gui/dependency-license-inventory-v0.0.3.md)。

新增三个记录域的真实 metadata 容量与后端现行保留预算，保留未知/真实零/过期采样及独立
清理后重读。没有读取正文、自动清理或修改配置，范围与未完成的逐项管理见
[容量契约](../gui/local-data-capacity-v0.0.3.md)。

本批云端完整前端验证通过，类型检查 0 错误/0 警告，生产构建成功。单线程 locked
workspace/all-features Rust 完整验证 1562通过/0失败/7忽略（GUI1288/0/5）；fmt、
workspace/all-targets/all-features check 与 Clippy 通过并保留既有警告。真实 Rust 许可
hook 的输入/摘要拒绝验证通过。一次并发全量测试的501条大快照夹具500ms超时，后续仅为
此大快照改用5s预算，再跑并发全量也为1562通过/0失败/7忽略；其他夹具和显式超时
预算不变，不删改安全断言。

32项受影响浏览器检查及14项修正文案/状态定位后的路由与本地容量检查通过。整批 Chromium
初跑452/464通过，12项均在旧规则页说明文字断言处失败，修正后已包含在上述14项复验中；
没有将初跑写成一次全绿。当前头的完整 CI、SDK 包、Simulator实际版本、产物摘要和未运行项以 PR 最新段为准；旧头
证据均为历史。Android API24/Galaxy S8 与 iOS16.4 的目标不变，未声称真实手机或 VPN
验收。没有修改 core、操作用户 Mac、创建 Apple 凭据、合并或发布。

2026-10-19 16:24 UTC：[GitHub 官方状态](https://www.githubstatus.com/api/v2/summary.json)记录 Actions/Pages/Webhooks 降级，当前头的
push/PR 共12条工作流仍 queued、未分配 runner。此时没有当前头的 SDK/App/APK 产物或
Simulator 执行证据；历史 iOS26.2 结果不能移作本批通过，等待外部服务恢复后继续核对。

## 2026-10-18：可安装 Debug 宿主

本批从 `8810cc15607f100d628e5c5e350bf359cdc33b93` 继续，保留 DNS 固定预览/事务保护。
新增独立开发包、临时 SDK 开发签名、Debug inspector、严格 Debug/Release 许可目标及实际 APK
校验链。构建使用打包前端且 `devUrl=null`；不依赖电脑开发服务器。详细操作见
[USB 指南](../gui/android-debug-usb-v0.0.3.md)。校验器单元使用替代工具，真实签名仅由实际
APK 的 apksigner 结果证明；证据收据不声称设备执行。最终 head、CI 和产物摘要在 PR 中追加。

最新 develop `408327df53554c28d5efeda3ece65535cabf783c` 与本分支存在10项真实冲突。
本批不合并、不重置；push 检查验证工作分支精确 head，不表示合并结果已通过。
无模拟器/真机、未运行 ADB、未生成持久访问凭据；VPN/内核/生命周期及四项许可缺口保留。

2026-10-13。客户端基线 `develop@fd198ba52711f018334860b8a6704546ccd349fd`，
工作分支 `feat/mobile-v0.0.3`，开发版本 `0.0.3-dev.202610131010`。
本次仅在云端实现和验证，没有在用户 Mac 修改、构建或测试。

本记录以交接的完整移动 PRD V3 为基准；HTML 只提供布局参考，没有复制其
模拟数据、事务定时器或假成功。此次是持续更新的客户端适配 draft，**不是完整移动
VPN 交付**。下列“通过”只指对应代码检查或替代 IO 的测试，不代表手机验收。

本记录保留基础阶段与第二阶段的历史证据。2026-10-13 后续诊断/许可/平台投影
整合的当前范围及逐项剩余条件见 [移动验收账本](../gui/mobile-acceptance-ledger-v0.0.3.md)、
[有界诊断](../gui/bounded-mobile-diagnostics-v0.0.3.md) 和
[构建依赖许可目录](../gui/dependency-license-inventory-v0.0.3.md)。最新 commit 的
CI 链接、计数与最终 APK 收据在同一 draft PR 描述和交付报告核对，不借历史成功替代。

## 2026-10-15：锁定版本的原始许可材料

本批从 `8c043cd53a831ac27237fbae9c46f21aeb452377` 继续。新增 npm/Cargo/Maven/Swift
精确身份的原文缓存，保存原始版权、NOTICE、标准条款来源及字节摘要；字体、旗帜与 CSS
按实际构建输入/输出记录，未安装或未使用的包不因 manifest 出现就声明打包。
Cargo release、Android merge 与最终 APK receipt 检查材料缓存新鲜度；自有 crate
明确引用实际 MPL 文件。具体缺失和共同条款边界见 [许可目录](../gui/dependency-license-inventory-v0.0.3.md)。

云端 `pnpm verify:frontend` 通过，Svelte/TypeScript 为 0 错误/0 警告；最终许可专项
31 项 Node 测试、12 项 Python 测试、Swift fixture 与原有清单断言通过。
真实 Rust build-hook fixture 验证匹配清单、过期缓存/本地材料、非法路径、正文篡改及引用拒绝，
并使用实际 Cargo build-dependency 版本，避免误取另一个 sha2 版本。
Rust fmt、workspace all-targets/all-features check、全特性 tests 与 clippy 通过；
测试 1116 通过、0 失败、7 ignored，已有警告仍保留。

新的真实 Gradle release collect 已通过，68 Maven 与 6 本地项目的实际归档/POM 摘要
和原文材料可合并。许可批次 `0d20849` 与 SDK 修复 `a55e84b` 的云端无签名 APK
已实际构建并通过 final receipt：50,117,939 字节、arm64-v8a、min API24 / target36、
`org.zerodenet.znetsink`，Native IO 已包含；500 项、484 份唯一正文、4 项明确限制。
APK SHA-256 为 `4dafb7a8b2c8f634cdcc313ed8b2d5f59a111951828df06977e472f3671818fa`。
后续日志源码改动不借此产物宣称已打包，其最终 commit 仍由对应 CI 重新构建。

`a55e84b` 的 Native documents CI 已成功完成 Android library、Swift policy 8 项测试
与真实无签名 UIKit Simulator SDK 构建（arm64/x86_64）。SDK 清单各 9 项、3 项明确缺口，
没有称作 iPhone app/provider 产物。之前 `0d20849` 的 Apple 测试错误依赖 runner SDK
自动发现，导致实际 UIKit 步骤跳过；修复将显式 metadata 与隐式发现隔离，并校验同时
给出的 SDK root 原始字节匹配，补入跨平台执行的 Darwin 分支 fixture。
没有 iPhone、Galaxy S8、VPN 服务/扩展和设备网络验收。

## 2026-10-15：宿主类型化的诊断日志正文

诊断文件使用 `znet.mobile-diagnostics.v3`。真实进程、调度、节点、测速、旧连接清理
和插件宿主路径通过不含自由字符串/任意 JSON 的封闭事件提供固定 `safeMessage`；原消息、
动态字段及未确认的身份仍排除。普通 RPC、Core 和 guest 日志不授予分类；来源不符、
旧 marker、未知或畸形持久分类只保留可读元数据，不升级为当前运行事实。

插件分类来自实际 Run/Uninstall、SDK typed Method 或调度调用点；generic invoke 固定
ComponentAction，名称碰巧叫 `sdk.SubscriptionApply` 或 `uninstall` 也不会借用其语义。
规则依赖缺产物只记录 ArtifactUnavailable；未使用的 provider 更新失败仍可通过真实配置
注入校验，不声称一定阻止配置应用。测速本地失败不标为远端不可达。

最终 focused 39 项、JSONL store 11 项及既有被引用 provider 缺失拒绝回归 1 项通过。
覆盖真实通用 invoke 的 8 个同名碰撞、6 个 SDK 类型映射、未使用 provider、RPC/普通入口
伪造、混合秘密 canary、source 冲突、JSONL 轮转重开与固定 preview/take。独立只读复核
已核对调用点、SDK desktop 条件编译和两项事实问题的修复。最终 Rust fmt、workspace
all-targets/all-features check、全特性 tests、clippy 及实际 license build-hook fixture
通过；tests 为 1129 通过、0 失败、7 ignored（45 组）。完整前端验证、后续权限/归属和
115 项移动语义回归通过。准确提交 CI 在交付前另外核对；旧 APK 仅证明前一许可批次，
不作为这批日志源码的手机验收。

分类持久化沿用应用私有日志；它不提供对直接改磁盘主体的密码学真实性，也不用于控制
内核、恢复旧任务或判断当前 VPN 健康。其他自由正文保持省略，仍有真机后台/分享等验收缺口。

## CI 后续：运行所有者回执与身份发布顺序

`00462b0` 的实际 isolated Proxy CI 揭示命令结果先唤醒调用方、再发布新 revision 的竞态；
立即读取身份再查询时可能得到 StaleIdentity。客户端 owner 改为先发布实际操作后的身份，
再交付原始成功或失败回执；保留正在关闭阶段、精确 CAS、原操作结果和不重放约束。
同步回执 waker 的确定性测试在旧实现失败、修复后通过；新增真实 Proxy 的 32 次模式事务
检查新身份、运行 revision、旧身份拒绝及失败命令不改变已确认模式。原 CI 用例另外重复
200 次通过。isolated workspace fmt、all-targets/all-features check、7 项全特性测试和
clippy（拒绝 warnings）通过；core 与锁文件未修改。原生 VPN 装配仍未启用。

## 本次代码

- 原生 Android/iOS 或窄窗口采用四固定底栏。高级管理由移动投影保留，不修改
  用户桌面 Lite/Pro 偏好。二级页保存来源，各底栏已访问的页面保留搜索、展开、
  草稿与滚动；工作区、配置和订阅编辑离开时确认未保存内容。
- 概览只有一个首屏统计入口，定位实际全局 Flow。统计工作区复用应用共享会话，
  无损 u64 累计与本次采样分开；当前速率未知显示 `—`，不同实例、配置、epoch
  和观测缺口重新建立基线。后台暂停 UI 查询，保留公共事件通道；前台重新发现。
- 节点页按配置顺序展示全部组，多组展开，搜索不覆盖用户展开记录；区分直接成员
  selector 与自动/其他组。组或单节点测速固定目标，停止保留 draining 事实，
  重试只取原任务失败项。节点、测速与模式调用增加客户端作用域前置条件。
- 配置与订阅为两个分段，路由、规则集和原生工作区保留二级入口。离线本地选中
  与运行态确认分开。沿用 Rust 配置、订阅、DNS、规则、持久化和应用链，未新增
  “移动 JSON schema”。未知字段、来源归属和原有权限仍由现有领域决定。
- 端点区分“清除运行覆盖”和“恢复来源”。后者移除客户端覆盖之前核对普通来源、
  已确认配置回执、资源身份及版本，先预览影响，再协调实际配置与端点回执；
  本地待应用、运行确认和持久化分别记录。缺少可信绑定或运行桥时不能宣称恢复。
- 统计只呈现所选范围实际提供指标的平面；没有虚构 Host identity。新 Packet
  页面使用 `zero-api` DTO、真实查询与关闭命令、固定目标确认、profile/实例/
  revision 校验、无损计数和回执核对；未知结果不会自动重放。
- 移动 Tauri 装配、私有容器和权限独立于桌面。没有移动运行桥时，拒绝桌面
  进程、代理、TUN、安装器、原始 IPC 和运行操作，不伪造可转发内核实例。
  发行页分别展示默认随包策略与实际缺失的移动内核产物。
- 新增宿主持有的 Android/iOS 系统文档插件，按用途限制读取和导出预算，原生
  路径/URI 不进入界面。普通配置导出需确认凭据风险；托管材料拒绝导出；取消、
  分享已接受及结果未知分别显示。诊断导出使用固定、有界脱敏快照，含版本/平台、
  安全日志分类、真实任务/捕获状态及具有可信来源的公共计量事实，原始正文默认排除。
- 设置迁移白名单、覆盖范围预览、原生 schema 与 DNS 全引用校验、版本/实例/
  来源前置条件及原子本地保存已接入；本地保存和运行应用分开，无自动 VPN 启用。
- 删除配置先固定对象、订阅关系和版本，明确替代或停止；没有自动选择首项。
  原生移动当前配置删除仍因 VPN 状态未知受阻，独立非当前配置可本地删除。

## 平台及浏览器边界

Android 最低系统明确为 7.0/API 24，Galaxy S8 仍是必须真机验证的设备。
API 下限、Rust target 编译和 320px 浏览器检查不能替代该设备的包安装及运行。

当前技术基线使用 Tailwind 4，前端构建显式以 Chrome 111/Safari 16.4 为目标。
系统版本与 WebView 引擎版本需分别检查；不声称 Android 7 原始 WebView 已兼容。
[Tailwind 官方兼容基线](https://tailwindcss.com/docs/compatibility)、
[Tauri WebView 说明](https://v2.tauri.app/reference/webview-versions/)。
iOS 主应用最低版本声明为 16.4，与前端基线一致；Native IO 库下限仍为 15.0。
已有 Xcode 无签名 Simulator SDK library 构建，尚无主宿主/扩展构建，
没有付费开发者账号、签名凭据、安装或真实 iPhone VPN 验收。

完整手机运行仍需 M01–M04。M01 生命周期和 M04 控制/事件已有公开 Rust 接口，
客户端服务所有权、随包打包、鉴权与持久恢复可独立推进；跨仓库缺口主要为
M02 外部 Android FD/iOS packetFlow 与 M03 全部外层 protect/bind/网络代次。
[跨仓库 API 清单与退出测试](../gui/mobile-platform-v0.0.3.md) 已给出 crate/trait 层次，
core 工作树只读且未推送。默认随包策略尚未落实为移动内核构建产物。

## 验证

诊断/许可/Host/发行检查整合后的最终本地源码验证（2026-10-13）：
workspace all-features **997 passed / 0 failed / 7 ignored**；fmt、all-targets/all-features
check 与 clippy 通过，既有警告保留。`pnpm verify:frontend` 完整通过，71 项移动
语义测试、17 项许可来源/材料/target/产物断言、0 错误/0 警告及生产构建；
完整 Chromium **203 passed**。Rust build hook 的实际编译 fixture 和 Swift pin/SDK/多架构
来源 fixture 通过；后者不是 UIKit 实际编译替代品。日志前缀 `/tmp/mobile-phase3-source-final-`
及 `/tmp/mobile-phase3-final-`。最新四组 CI 尚需按本轮新 commit 单独核对。

本地实际 Tauri CLI 无签名 arm64 APK 为阶段证据：47,938,295 字节，API24/target36，
SHA-256 `b37ec4e58ab46915d1382cb9d4bf0febb187094f7b4d833f79cbba5a1f4877b1`。
真实 fixed asset、frontend SHA 和 `libgui_lib.so` 已核对，496 条记录/313 份去重材料/
69 项明确缺口（计数随 toolchain/target 变化）。诊断来源最后修正发生在此构建之后，
因此此包不能称为最终 PR head 产物。最终 head CI 会重建并上传 APK 和外部收据，
不得用该阶段包或旧 commit 的绿灯替代。

| 检查 | 本次结果 | 不能据此推断 |
| --- | --- | --- |
| `pnpm verify:frontend` | 全部现有回归、新移动/Packet 回归、类型检查及生产构建通过 | 手机服务与真实内核转发 |
| `pnpm check` | 0 错误、0 警告 | 系统 API/键盘已在手机验收 |
| Rust fmt、workspace check/all-targets/all-features | 通过 | iOS 宿主/扩展链接与签名 |
| Rust workspace/all-features 测试 | 基础阶段 899；新增管理/文档改动后 955 passed、0 failed、7 ignored | ignored 的真实二进制/平台测试已运行 |
| Rust workspace/all-targets/all-features clippy | 通过，保留既有及测试代码警告 | 严格 `-D warnings` 已通过 |
| 共享 Rust Android/iOS aarch64 target check | 两个共享 crate 均通过 | Tauri/iOS 主宿主或扩展已构建 |
| Android Rust 主宿主 API24 check | NDK r27d/API24 clang，`--lib --target aarch64-linux-android --locked` 通过；40 项未调用桌面代码等警告 | APK、签名、安装、TUN FD、S8 或 VPN 验收 |
| Chromium 浏览器 | 基础阶段 171；新增文档/迁移后全套 175 通过、3 项截图因磁盘不足失败，清理后原 3 项全部通过；另加 5 项实际删除 Dialog、3 项系统返回/页面恢复及 3 项实际关于页测试通过 | Android 原始 WebView、WKWebView 或手机系统返回 |
| WebKit 本地运行 | 未运行：当前云端浏览器下载被网络网关拒绝（403） | iOS/WebKit 验收通过 |
| 原生文档库 | 实际 Android 库编译、11 JUnit 与 AAR 通过；Swift Foundation 8 项通过 | Files/分享、UIKit SDK 或设备验收 |
| Android 无签名应用 | 实际 arm64 Release APK 构建成功；最低 API24，检测无签名；最后源码仍须按 commit CI 复核 | 可安装签名包、Galaxy S8、VPN 服务或随包内核 |

浏览器测试以真实生产组件和明确的替代 IO 执行；测试 fixture 不进入产品模块。
实际 Unix IPC 的 Packet 测试证明查询/关闭 DTO 与回执处理，仍不是网络转发真机证据。
详细云端日志保留于 `/tmp/mobile-frontend-verification.log`、
`/tmp/mobile-backend-tests.log`、`/tmp/mobile-cargo-check.log`、`/tmp/mobile-clippy.log`、
`/tmp/mobile-android-host-check.log`、`/tmp/mobile-browser-full-final.log`。
后续 Rust 修复全套证据为 `/tmp/mobile-final-rust-{fmt,check,tests,clippy}.log`，
升级事务回归为 `/tmp/mobile-upgrade-transaction-regression.log`；
测试 runner 的 Linux PowerShell 语法/C# 编译/参数转发证据为
`/tmp/znet-windows-runner-pwsh-validation.log`；插件示例回归为
`/tmp/mobile-plugin-example-checkout-regression.log`。

## 草稿 PR 与 CI

[Draft PR #46](https://github.com/zerodenet/znet-sink/pull/46) 合入目标为 develop。
首个代码提交 `f18f72e7f7ad10d0708dbcef205434a362de87f8` 的
[移动宿主 CI](https://github.com/zerodenet/znet-sink/actions/runs/38382760624)
中 Android API24 主宿主及 Android/iOS 共享 Rust 三项均成功。
[浏览器 CI](https://github.com/zerodenet/znet-sink/actions/runs/38382760521/job/115246989500)
Chromium/WebKit 共 342 项通过；本地 WebKit 未运行与此 CI 结果分别记录。

同一提交 Windows 客户端测试在启动前返回 `0xc0000139`
(`STATUS_ENTRYPOINT_NOT_FOUND`)，未执行测试，不能计为通过。
后续保留 Windows2025 全部检查，增加依赖入口诊断及隔离缓存的 Windows2022
对照。两套环境均复现相同加载失败；Python 探测已有 v6 Common Controls 激活上下文，
因此没有直接缺失入口，不足以证明 Cargo 测试 EXE 可加载。依赖中的 rfd 启用
`common-controls-v6` 并静态导入 `TaskDialogIndirect`，与
[Tauri 上游的库测试 manifest 缺失问题](https://github.com/tauri-apps/tauri/issues/13419)
吻合。后续测试 runner 在执行前读取 `RT_MANIFEST #1`，只为缺失 manifest 的
Cargo 哈希测试 EXE 用 Windows SDK `mt.exe` 嵌入最小 Common Controls v6 依赖。
现有 manifest 保留，全部测试参数及真实退出码继续传递；发行构建不使用该 runner。
`5187eb2` 的两套 Windows CI 已证实 manifest 原本缺失且完成嵌入，各实际
执行 623 项成功测试；随后暴露同一个升级备份断言失败，不能计为整项通过。
Windows 将“普通文件/receipt.json”的读取错误归类为 NotFound，旧逻辑会忽略
损坏的 `upgrade-*` 条目。后续修复先检查保留前缀的备份类型，并要求持久回执；
缺失或损坏时明确要求恢复，无关普通文件和目录继续兼容。新增两项回归，
事务 13 项及本地 workspace/all-features 899 项通过；`d391bfd2` 的两套 Windows GUI 主测试均通过，后续 workspace 在插件示例
入口摘要校验失败。隔离的 `core.autocrlf=true checkout-index` 重现：示例 JS
变成 CRLF，与 manifest 的 source_sha256 不符。补齐示例目录的 LF 属性后同样
checkout 保留声明摘要，原离线打包/安装/执行测试通过。没有更改摘要或校验逻辑；
基础阶段 head `ded57c37e46c01ab30cc4c09a90c6ca3c2030889` 的 Windows 完整
workspace/readiness 及其他 CI 已于 2026-10-13 12:15 UTC 核对全绿：
[主 CI 14 项](https://github.com/zerodenet/znet-sink/actions/runs/38393241966) 与
[移动 Rust 3 项](https://github.com/zerodenet/znet-sink/actions/runs/38393241982)。
后续设置迁移、文档 IO、删除与来源恢复是新的改动，必须按新提交单独核对 CI。
历史 PR #44 的合并提交 `7bf9337aa6a71f93acce4d9761a523946bf4e2ab`
不是本次 develop 基线的祖先。逐处比对确认相关 LF、备份文件守卫及库测试 manifest
修复未带入，而网络诊断 fixture 已有等价内容。本次按独立补丁修复，不合入 main
发布变化。Windows workspace 保留全部测试并使用 `--no-fail-fast` 收集其余套件
失败；失败仍是非零回执，构建依赖按 runner 隔离缓存。前端 CI 补充已验证的
桌面 shell 回归。

两个镜像名称来自 [GitHub 官方 runner 列表](https://github.com/actions/runner-images/blob/main/README.md)。

## PRD 验收状态

| PRD 范围 | 客户端完成部分 | 尚未通过的部分 |
| --- | --- | --- |
| A01–A02 | 四底栏、移动菜单、320px/字号/安全区样式、单个统计入口与返回测试 | 手机安全区、系统返回、软键盘真机 |
| A03–A04 | 明确状态/不可用反馈，模式作用域守卫及确认链复用 | 系统 VPN 授权、接管、恢复、模式真实移动提交 |
| A05–A07 | 20 组浏览、类型区分、局部测速、draining、失败项重试与失效守卫测试 | 原生服务持有任务、进程恢复与真实协议测速 |
| A08–A14 | 两分段、编辑草稿保护、系统文档 IO 接线、固定删除替代/停止预览、设置迁移白名单/校验/CAS；规则/DNS/工作区领域复用 | 当前移动配置删除需服务状态，系统选择/保存/分享设备验收、完整工作区及全部表单手机验收 |
| A15–A17 | 端点运行覆盖清理、可信来源恢复链与待应用/未知回执；统计平面、epoch/u64、现有固定目标清空链 | 原生端点桥、来源恢复真实运行验收与独立外部 writer 的 config/generation 原子前置条件、移动 Peer/Host 后端 |
| A18–A19 | 真实 Packet 控制链、完成及实际IPC活动筛选导出、捕获预算/清理/元数据恢复账本、固定脱敏快照及原生回执 | 自由日志安全分类、中断会话最终事实及平台验收 |
| A20–A22 | 固定插件入口与不可用原因，宿主底栏不交给插件 | 移动 VM/SDK IO/来源授权、敏感加载、安装更新卸载与真机隔离 |
| A23–A24 | 两独立真实发行目录/渠道/说明入口、绑定策略与缺失产物区别、实际应用元资料、主动外链/复制、生成构建依赖目录与可读材料/缺口 | 内核随包产物、移动重启/安装、缺失许可资料、iOS 主应用目录与外链手机验收 |

没有合并、发布、改分支保护、购买账号或创建签名凭据。

## 2026-10-14 独立客户端协调检查

本轮新增实际 IPC 活动连接导出、捕获恢复账本、原生前台订阅到期协调、共享 Rust 控制桥与只读检查、共享监听取消/预算/缺口及未知事件脱敏。依赖以根 [todo.md](../../todo.md) 增量维护；core 只读且远端 develop 仍为 e74561f。没有把 C FFI 作为 Rust 嵌入生命周期前置条件。

云端本地 workspace/all-features 测试 **1045 passed / 0 failed / 7 ignored**，fmt、workspace/all-targets/all-features check、clippy 通过（编译警告保留，含未在桌面装配的原生调度路径）。前端完整验证及生产 build 通过，svelte-check **0 errors / 0 warnings**，原移动语义套件76项另加只读桥回执3项；依赖许可17项。完整 Chromium 回归 **211 passed**；系统 WebKit 仍以同提交 CI 实际执行结果为准。

新增真实 Unix IPC 测试验证查询身份前后变化、关闭单监听不终止 pending control/另一 observer、帧/事件/请求预算与秘密脱敏。独立 HTTP/SQLite 测试验证前台同步、修改/手动同步竞争、恢复未知屏障和304拒绝保留材料。这些夹具不等于生产 Zero/手机内核、原生生命周期或真实分享验收。

该次检查时，移动 Proxy/服务注册及随包可运行内核、TUN/保护、后台 OS 调度、304条件缓存语义、iOS主应用/provider与真机转发仍未完成。不创建签名凭据或购买账号。后续条件同步实现见下节；最终 head CI/原生构建收据继续在 PR 描述更新，不借前一提交成功替代。

## 普通订阅条件同步与独立 Proxy owner

普通订阅现在复用共享 HTTP executor，持久 ETag/Last-Modified 与实际来源和有效材料绑定。
真实 HTTP200→304 测试确认 304 不替换材料、不提交运行配置、不修改选择或覆盖；暂停自动
同步仍允许用户手动同步。损坏条件记录有固定来源及版本的显式清理确认，SQLite CAS 仅删除
该来源的条件元数据，不下载、不应用、不解除运行提交结果未知屏障。额度缺失、旧数字 DTO
超出安全整数及来源未提供额度采样时间分别说明；没有合成零值或健康状态。

具体 [RuntimeOwner](../../src-tauri/crates/mobile-runtime/src/owner.rs) 使用固定 core e74561f
的公开 Proxy/RunningProxy，位于独立 workspace，未链接进主应用或 APK。专用 OS 线程及
Tokio executor 持有完整生命周期；真实 Linux TCP 回环在原 UI executor/观察句柄销毁后
继续转发，停止等待实际 shutdown 和 executor disposal。默认及 all-features 各 **5 项通过**，
fmt、check 和严格 clippy `-D warnings` 通过。这不证明 UDP、DNS、所有协议或手机转发。
core 所需八个 vendor patch 使用同一不可变版本，未替换主桌面 workspace 的 TLS 依赖。

云端实际 Android API24 NDK 完整 owner check 在固定 core 的 zero-tun 工厂报 **E0282**：
非桌面分支 `io::Result<impl TunDevice>` 只有 Err，无法推断 concrete 返回类型。
已记录 [CORE-05](../../todo.md)，没有修改 core 或缓存源码，没有用 EngineHandle 或假 Linux
cfg 绕过。iOS owner SDK 检查未运行。PR 的完整 owner iOS job 及 Android owner step 仅保留
手动审计入口，正常 PR 中跳过，不计为目标编译成功。已有轻量共享 Rust、原生文档库和普通
Android 主宿主/APK 检查继续执行；主应用没有实际 owner/TUN 接入。

用户已确认原生 VPN、内核接入及宿主生命周期装配后置；这只改变当前实施顺序。
后台 OS 调度、普通订阅结果未知的持久权威核对/解除入口、随包可运行内核、原生事件桥、
iOS 主应用/provider/App Group、插件执行边界及手机验收仍未完成。

本轮最终云端 Rust workspace/all-features **1057 passed / 0 failed / 7 ignored**，fmt、
all-targets/all-features check 与 clippy 通过，现有编译警告保留。独立 owner 的五项测试与
主 workspace 分开计数。证据为 `/tmp/mobile-round4-final3-{check,tests,clippy}.log`、
`/tmp/embedded-mobile-runtime-{tests,allfeatures-tests,clippy,android-check}.log`。
最终 `pnpm verify:frontend` 与生产 build 通过；移动语义 **84 项**、许可断言 **17 项**，
svelte-check **0 errors / 0 warnings**。完整 Chromium 回归 **214 passed**，包括实际订阅
组件的未知额度、暂停后的手动同步及固定来源条件记录清理确认。证据为
`/tmp/mobile-round4-final3-frontend.log`、`/tmp/mobile-round4-final-browser.log`。
本地 WebKit 未运行，同一提交 CI 双引擎结果和新原生产物收据在 PR 更新，不借用旧提交。

提交前只读复核客户端 develop 仍为 fd198ba5；core 远端 develop 已更新到 c2a72f6，
增量未解除移动接口及 CORE-05，客户端与独立 owner 依赖仍固定 e74561f。只 fetch/read
core 元资料，审计工作树仍干净，无 source/缓存修改。

### Windows 已发布内核回归

`94cfbc7` 的双引擎浏览器 **428 passed**，原生三个工作流均成功；主 CI 的 Windows2025
已发布内核测试有一次真实失败：占用端口场景在仅等待 IPC health 持续300ms 后提前返回。
Windows2022 同场景通过，不能拿对照成功掩盖失败。源码核对确认 IPC/Engine health 不是
listener orchestration 回执，生产 managed host 已使用空 bootstrap，随后等待
`config.apply_runtime` 确认才标记 Running。

真实回归改为上述完整生产顺序；正常场景要求 runtime-only 确认、实际监听及 parent pipe EOF
后资源释放，占用端口场景仍要求实际运行应用被拒绝，没有增加睡眠来制造就绪，没有删测试。
云端从正式 v0.0.1 获取 Linux archive 并验证官方 SHA-256
`1ab96e271219b7c5f03310260c6b823c78ee20e776e33af79d5fe0a6253fce82`，两项真实测试通过；
常规 readiness 五项通过。日志为 `/tmp/mobile-final-readiness-{regression,real-linux}.log`。
Windows 两环境及最终提交的所有工作流结果继续在 PR 描述核对，不借旧提交的浏览器或 APK。

### 插件宿主边界与订阅只读核对增量（2026-10-14）

同一分支和 draft PR 继续交付客户端功能；没有修改 core 源码/缓存、签名、发布或原生 VPN 装配。
客户端 develop 只读复核仍为 fd198ba5，core develop 仍为 c2a72f6，固定依赖仍为 e74561f。

- 真实 QuickJS 执行移到共享有界 worker：2 在途、32 排队、5秒排队期限，原执行/SDK预算与授权TTL不延长；同一原租约跨执行、 owning 提交与交付。停止、撤权、stop/start ABA和宿主关闭拒绝旧启动/结果，在途IO真实 draining。
- 签名页面材料、HTML摘要、组件registration/revision和宿主文档token原子绑定；8个RPC只在桌面注册。固定窗口/channel/代次、前后台关闭/重建、文件选择前后核对和原租约SDK执行已实现。要求Process隔离仍拒绝；VM文件路径不能冒充用户选择，可信票据接入前明确 unsupported。
- 宿主先写完整受控head/CSP/bootstrap，任何guest字节在其后；URL frame导航由parent CSP拒绝。实际本地HTTP探针覆盖pre-head脚本、pre-head图片和注释fake-head，均收到0请求并确认CSP阻止及正常SDK可用。畸形同来源请求也消耗512次预算。
- 普通订阅按固定来源版本/材料/当前IPC身份只读核对，保留无损u64；旧记录没有候选/持久回执时始终result-unknown、canResolve=false。不清屏障、重放、下载、配置写入或创造运行确认。

最新源码云端 Rust workspace/all-features **1089 passed / 0 failed / 7 ignored**，fmt、all-targets/all-features check和clippy通过，编译警告保留。
日志 `/tmp/mobile-phase5-final3-{fmt,check,tests,clippy}.log`。真实已发布Linux v0.0.1内核的两个启动/应用/释放回归通过，
日志 `/tmp/mobile-phase5-final-real-linux.log`；这是已发布桌面内核，不是当前固定core或手机转发验收。

最终前端完整验证/生产build通过，移动语义 **107项**、许可 **17项**，类型检查 **0 errors / 0 warnings**，
日志 `/tmp/mobile-phase5-final2-frontend.log`。真实签名包Host页面/租约/材料回归8项通过；Host执行/ABA/取消及VM文件准入5项由workspace全集覆盖。
实际Frame相关13项Chromium回归通过（12项通道/CSP场景加既有管理流程），日志 `/tmp/plugin-page-csp-fix-final.log`；最终完整Chromium回归另行记录。

本地WebKit缺少执行文件，未运行；最终commit双引擎和原生产物证据以同一PR描述记录，不借旧head。
真实系统文件提供者、OS浏览器、原生WebView、第三方HTTP、移动SDK/敏感加载、进程/OOM隔离和手机VPN均未验收；
原生服务/扩展注册及CORE-01/02/05未解除。普通订阅未知结果的持久权威解除和系统后台调度仍待实施。
详见[执行边界](../gui/mobile-plugin-execution-boundary-v0.0.3.md)、[页面通道](../gui/plugin-page-channel-v0.0.3.md)、[只读核对](../gui/ordinary-subscription-reconciliation-v0.0.3.md)及根[todo.md](../../todo.md)。

上述最终源码完整 Chromium 回归 **228 passed**，日志 `/tmp/mobile-phase5-final-full-browser.log`；没有跳过或重试通过项。

类型化错误收尾补丁随后补齐：原租约的预算、deadline、撤权、过期、关闭及繁忙不再经过通用包/网络错误转换器；直接保留现有SDK canonical code，并用固定文案排除任意原消息/details。实际资源预算、租约超时和撤权/关闭分类使真实Host页面聚焦回归增至11项，Host执行/文件准入为6项。
补丁源码 Rust 全集 **1093 passed / 0 failed / 7 ignored**，fmt/check/clippy通过；最终前端完整验证/build通过，移动语义 **108项**，许可 **17项**，类型检查 **0/0**。
日志 `/tmp/mobile-phase5-typed-final-{fmt,check,tests,clippy,frontend}.log`。同一补丁的完整Chromium运行记录在 `/tmp/mobile-phase5-typed-final-browser.log`；最终终态、双引擎CI和unsigned APK收据以最新head的PR记录为准，634dd29的工作流/产物只算历史证据。


### 4293774 双引擎首次加载回归

上述类型化错误补丁的系统 Chromium 全集实际通过 **228项**；同一提交 GitHub Actions
使用的 Chromium/WebKit 则 **433 passed / 23 failed**，插件 iframe 首次内容没有呈现。
主 CI 其他13项全部通过，不能把本地不同版本浏览器或其他检查通过称为双引擎验收。
移动 Rust 与原生文档工作流通过；unsigned APK 49,552,095 bytes / arm64 / API24，
文档 payload 与许可 asset 检查通过，artifact 11872238395。此提交产物仍没有装配 Proxy/TUN。

精确 Playwright 浏览器下载及失败 trace artifact 本地获取受到环境 CDN/domain 403 拒绝，
不是自动批准审查拒绝，没有修改依赖或绕过网络限制。首次加载与安全握手修复后的新提交
必须重新跑实际双引擎 CI；此处保留失败历史，不能借用上一个 head 的通过记录。


首次加载修复改由当前WindowProxy与新channel的真实bootstrap建立，ready/load/host ack
完成后才接受业务请求；早到空白页或旧iframe回调不建立通道。初始化后的load或pagehide
继续关闭原文档，旧授权、导航和网络CSP断言没有放宽。新增三个实际Frame回归覆盖早到
空白页、多次/延后的宿主load及握手前rawguest业务调用（拒绝且计入预算）。
本地聚焦 **16/16 Chromium** 通过，页面通道Node **8/8**，前端全集验证及生产build通过，
移动语义108项/许可17项，类型检查0/0。日志 `/tmp/plugin-page-handshake-final.log`、
`/tmp/plugin-page-handshake-final-node.log`、`/tmp/mobile-phase5-handshake-final-frontend.log`。
这是系统Chromium154的实际结果；尚不能证明CI Chromium151/WebKit2336首次加载已修复。
Rust及原生配置没有变化，前述1093项验证覆盖同一Rust源码。新head完整CI需要重新核对。


5b95adf 将真实握手与三个顺序回归推到原draft PR 后，标准核对确认初始parent load和
postMessage处于独立任务源，不能用ack往返证明所有load已到达。其CI仍需观察，但不论
终态都继续消除这个歧义：后续load需要原文档对新的host challenge做核对；核对中暂停
调用/交付，pagehide或无法核对关闭原通道。标准依据与最终行为见页面通道文档。


中间提交 **5b95adf** 的实际 CI 双引擎 UI 为 **462 passed / 0 failed**，运行
38518311905 / job115702025361，证明保留的parent/child CSP在CI Chromium/WebKit可执行，
无需放宽URL来源。该通过仍不替代随后消除任务源排序假设的最终源码验证。

最终同文档核对使用每文档bootstrap私有随机proof和每次新host nonce；原window/channel/
epoch同时核对，核对中原请求保留在已有八个在途名额内，不新建业务队列/ID或重放。
返回内容等待前单项1MiB与并发持有4MiB限制；恢复后核对原文档授权。已确认的configure
收据沿其已有owning路径消费同一host快照后关闭旧页，不用变化后的review再授权。
所有匹配原窗口/channel的业务与控制消息共同计入512次/256KiB输入预算。

冻结源码的实际Frame聚焦回归 **22/22 Chromium**、通道Node **8/8**、类型检查0/0，
日志 `/tmp/plugin-page-document-proof-frozen{,-types,-node}.log`。覆盖晚到初load、错误/无回应、
原授权撤销、固定请求等待和旧结果拒绝、自身失效配置收据、捕获监听顺序与控制消息预算。
真实销毁可令pagehide消息source为null；不放宽原窗口校验，实际替换走五秒有界核对关闭，
明确报告文档未确认。当前有效来源的pagehide则立即关闭；不宣称所有浏览器销毁都立即可验。
第一轮夹具的evaluate传参/消息阻断错误和五秒断言竞争已修正，失败不计为通过。
原backend/Rust/原生配置没有改变，1093项Rust证据仍针对同一源码；最新完整前端验证、
最终head的全量双引擎CI及unsigned APK收据在原draft PR继续核对。

最终同文档核对源码 `pnpm verify:frontend` 全集及生产build已通过：移动语义108项、许可17项、类型检查0/0；日志 `/tmp/mobile-phase5-document-proof-final-frontend.log`。本轮未再次运行未改变的Rust全集，前述1093项源码证据与最终head CI各自说明。


79a754a 实际CI为 **473 passed / 1 failed**：唯一失败是WebKit拒绝外部URL后保留原srcdoc，
旧测试误要求其必须关闭。13项其他主CI检查与移动Rust/原生文档/APK工作流均成功；
artifact11874112866为该head的49,551,687-byte unsigned/API24/arm64宿主，不能借作后续head。
测试需要验证实际CSP拒绝/0请求与真正文档替换的旧权限/结果失效，不能把拒绝尝试误作已导航。

随后补齐入站私有文档证明与出站document-bound MessagePort。仅检查WindowProxy及一次
proof不能保证异步回执不被后续文档接收；私有port在bootstrap/guest边界前建立并固定捕获，
源窗口校验仍保持。此增量的完整验证和最终head CI继续在原draft PR核对。


冻结后的child-created私有port源码：实际Frame/既有签名流程 **23/23 Chromium**、通道
Node **9/9**、类型检查0/0、差异检查通过。真正文档替换回归保持相同WindowProxy，
延迟parent load并暂扣旧closing，确认新doc用旧已知channel/错误proof不能调用，旧native
收据在parent load之前也不能被新Window接收；之后有界关闭且不重放。
公共SDK v1调用和Call/Reply DTO未变，port/proof不经公开context提供。
日志 `/tmp/plugin-page-private-port-{final-browser,final-types,node}.log`。
最终 `pnpm verify:frontend` 全集和生产build通过：移动语义109项（包括新增分离入站身份/
出站目标回归）、许可17项、类型0/0；日志 `/tmp/mobile-phase5-private-port-final-frontend.log`。
23项回归的首轮HTTP server收尾因闲置连接挂起，已显式清理连接；CSP/0HTTP/真实替换
断言均保留。新的准确head全部CI和APK收据继续在原draft PR记录，不借79a754a成功产物。

## 恢复执行时的已确认基线

`20603815828c3adafb9e2f5ba2a45a82891f062a` 的四组工作流均成功：
主 CI `38524928261` 的14项检查通过，其中实际 Chromium/WebKit UI job
`115722938791` 为 **476 passed**。移动 Rust `38524928250` 为4项成功、
unsigned iOS Proxy owner 检查1项跳过；Android Rust host中完整 Proxy owner 的
NDK步骤也明确跳过。原生文档 `38524928249` 的3项检查通过。

Android host `38524928259` 成功，匹配同一提交的产物 `11875627777`
（`android-unsigned-host-evidence`）包含未签名 arm64-v8a APK：
49,551,399字节，应用标识 `org.zerodenet.znetsink`，minSdk24、targetSdk36，
原生文档库与payload检查通过。18,641,166字节是artifact ZIP大小，不是APK大小。
这仍是host证据：没有装配实际移动 Proxy owner/随包可运行内核，不证明Galaxy S8
或iPhone VPN、TCP/UDP/DNS、撤权、网络切换、锁屏恢复通过。

后续普通订阅持久回执及本地发布恢复属于新的改动，需独立运行验证并核对新的head CI。

## 普通订阅 confirmed receipt 本地恢复：客户端验证

本批增加固定来源版本及恢复记录的明确本地保存操作。首次读取没有写入；只有持久
权威回执及全部作用域核对通过才提供保存。缺回执、旧未知记录和失效身份保留阻塞。

前端完整 `pnpm verify:frontend` 和生产build通过：移动语义 **115项**，依赖许可
**17项**，类型检查 **0 errors / 0 warnings**，日志
`/tmp/mobile-phase6-recovery-frontend.log`。生产订阅服务Node契约 **22/22**；实际
订阅组件 Chromium154 **11/11**（含旧7项、新恢复4项），日志
`/tmp/mobile-phase6-recovery-browser.log`。覆盖固定source/recoveryId、pending期间
重复/退出阻止、失败保留证据且不重放、prepared及stale保持只读。

这些浏览器断言使用明确的服务fixture验证UI，不充当真实内核应用或真机网络验收。
本地标准Playwright浏览器/WebKit仍未运行；新提交双引擎结果以同一head CI为准。

最终客户端 Rust `cargo test --workspace --all-features --locked --no-fail-fast`
**1116 passed / 0 failed / 7 ignored**（45个结果组）；fmt、workspace
all-targets/all-features check 与同范围 clippy 均通过，Rust/Clippy警告保留。
日志 `/tmp/mobile-phase6-recovery-{fmt,check,tests,clippy}.log`。其中新增SQLite
账本13项、业务恢复9项、真实Receipt结构指纹校验1项；现有实际Unix socket核对
继续证明仅发typed runtime查询，无apply、HTTP或TUN操作。

覆盖schema1/2→3迁移保留关系及能力存储、准备屏障/确认落盘/关系发布/完成阶段、
容量和32项保留预算、SQLite触发器故障整批回滚、损坏候选和摘要、同毫秒编辑、
规则集组合漂移、不同实例/revision、应用设置投影失败、重开及当前进程已知阶段
缓存协调、原版本/原ID重复完成，以及Complete已经提交但结果交付失败时禁止旧
内存回写。首次全集失败源于旧schema1迁移测试仍断言2；已更新为3并检查恢复表为空。
最终全集以上述1116项结果为准，不用早期22项聚焦结果替代。

本批仅在不需外部桌面系统代理重定位的普通活动配置变更中记录可恢复事务；其他
同步仍保留原实际重定位/协调链路。未恢复HTTP validator、没有持久确认回执及
移动owner未注册时的运行核对/恢复均不宣称完成。独立原生Proxy、真机VPN及发行
验收继续遵循现有todo；本批没有修改core来源或缓存。


## 客户端草稿、任务作用域与安全导出增量

本批在同一个 draft PR 继续完成三类可独立验证的客户端工作：

- DNS、VPN/TUN、运行网络和测速参数设置保留未保存草稿，离开需继续编辑或明确放弃；
  提交期间阻止切换，失败保留输入，刷新不覆盖脏表单。
- DNS/路由工具任务将历史与当前结果分开；共享客户端增加 `scopeGeneration`，对配置或
  实例的 A→B→A 变化仍使旧资格失效。普通观测 revision 不会清掉当前结果。开始/停止
  携带原作用域及客户端 revision，由 Rust 在分配任务 ID 或取消前核对。前后台和事件
  恢复重读实际宿主记录，旧完成记录不会重放 Fake-IP 副作用。此处不替代 Zero 的远端 CAS。
- 日志复制、当前筛选导出以及桌面旧诊断入口均使用有界安全投影。筛选固定实际 ID/时间，
  搜索正文不进入请求或导出；未经分类的原文/字段明确省略。预览计划绑定用途、作用域和
  generation，单次消费，交付前后核对；系统取消与结果未知保留实际事实。旧桌面接口
  没有 planId 时拒绝，错误恢复按钮先进入关于页预览，不直接写文件。

实际本地完整 `pnpm verify:frontend`（含生产 build）通过，类型检查 **0 errors / 0 warnings**。
Rust fmt、workspace all-targets/all-features check、全特性 tests 和 Clippy 通过；测试为
**1150 passed / 0 failed / 7 ignored**。编译及 Clippy 既有警告保留。日志
`/tmp/mobile-client-followup-{frontend-final,fmt,rust-check,rust-tests,rust-clippy}.log`。

新增/相关语义验证：任务作用域23项、原任务归属13项及实际 Tauri wrapper/观察器3项；
设置草稿4项、桌面诊断2项、复制安全投影4项。实际组件 Chromium 回归为设置39项、
日志导出4项、桌面诊断8项、工具作用域3项，全部通过；采用明确服务 fixture，不充当
真实内核应用或手机转发证据。系统 Chromium 与 CI 使用的浏览器版本分别记录。

最终推送提交的完整双引擎及四组工作流需另外核对，结果记在同一 PR。
上个 `1433527` 的 CI/产物仅为历史。Android API24 与 Galaxy S8 契约、iOS 无签名/
真实设备限制不变；四项依赖许可缺口仍明确保留。台账另列三项独立客户端后续工作：
端点统计定位与来源返回、Packet 分页、工作区离线草稿与来源冲突/托管只读准入。
它们不归到原生 VPN 后置范围，也不宣称本 PR 完成完整 PRD。


最终复核另复现超安全整数任务 ID 可被回送停止的边界；补丁拒绝超范围任务/作用域身份，
显示明确未确认/不可表示状态，不保存舍入后的控制目标。任务语义增加到26项，覆盖恢复、
事件、启动回执及两类数字作用域与越界u64。Rust源码未变化，前述1150项仍覆盖同一后端；
补丁前4761d57的在途CI仅作历史，新head另行核对。

安全整数补丁冻结后，任务语义 **26/26**、实际组件 Chromium **4/4**、完整
`pnpm verify:frontend` 和生产 build 再次通过，类型 **0 errors / 0 warnings**；
独立只读复核及独立重跑26项也通过。日志
`/tmp/mobile-tool-id-boundary-{node,browser,frontend}.log`。首次容量fixture误用宿主不会
分配的0号ID；改为合法1号后仍验证128条及活动优先，不放宽生产准入。


### 双引擎 CI 时间预算

`9c86efb` 的主 CI 双引擎 job 在20分钟预算处被取消：实际546项、单worker，
已执行Chromium273与WebKit229（共502），44项未完成，日志无断言失败或重试，
不能写成全量通过。job116249712447于04:37:19Z启动、04:57:38Z结束。
其他已完成工作流与50,363,575字节unsigned APK只作该head历史证据。
后续仅将该job预算增至30分钟，仍执行全部用例、原引擎及单worker，不改断言或运行代码。
最终新head的完整结果与产物另在同一PR核对。

## CLIENT-05/06/07：统计来源、真实分页与本地工作区

本批继续同一个Draft PR #46，develop仍为`fd198ba52711f018334860b8a6704546ccd349fd`，
不修改core源码或本批依赖锁，不接入后置的原生运行owner。

- CLIENT-05：实际registry定位共用统计页；固定来源及客户端代次，返回和端点来源的
  reset均重新核对。对象替换、删除、ABA及晚回执不能恢复旧详情；无真实scope不兜底。
- CLIENT-06：真实offset/limit/next_offset，500/500/1分页与末项、当前页刷新及精确重试；
  本页筛选和实时读取有明确说明。原物理目标的进程内关闭屏障跨页/重挂载保留，坏回执
  和未知错误不释放，不重复发送。进程重启持久恢复仍未实施。
- CLIENT-07：真实原生schema/引用校验、Rust JSON文本无损u64、独立SQLite本地保存、
  来源/关系图/AppConfig/规则及客户端代次CAS；托管/受保护来源不返回普通正文。
  脏草稿不被刷新覆盖；唯一导航guard在提交中阻止返回/切tab，完成不自动离开。
- 运行提交固定候选、完整捕获参数及原连接/identity，保持TUN命令15秒响应预算和权限、
  DNS及Windows准备。快速路径在配置提交前后核对真实TUN；未确认原运行材料不自动
  apply本地旧配置或重建旧TUN。纯本地捕获漂移明确拒绝；late drift及缺回执保留屏障。
  实际运行回执与本地发布分阶段，不用历史相同digest确认新事务或盲目整表恢复。

最终本地`pnpm verify:frontend`及生产build通过，类型检查**0 errors / 0 warnings**。
Rust fmt、workspace locked all-targets/all-features check、全特性tests及Clippy通过；
实际**1181 passed / 0 failed / 7 ignored**，45组结果，编译/Clippy非阻塞警告保留。
忽略项仍包括需外部Zero二进制/真实双版本capture/实时订阅的测试及文档测试；不写成已运行。
日志：`/tmp/client050607-frontend-guard-final.log`、`/tmp/client050607-final-rust-{fmt,check,tests,clippy}.log`。

针对性Node：端点与统计59项、Packet18项、工作区及激活24项。
实际Chromium：端点统计17项、Packet320px8项、工作区320px6项均通过；整批先合跑29项，
唯一导航guard修复后工作区6项再跑。分别使用明确fixture，不作为真实手机转发证据。
Packet Rust18项、工作区真实Unix IPC九模式、延迟TUN回执及三项发布代次并发保护包含
在上述最终Rust全集内。完整Chromium/WebKit及四组工作流以本批最终head的PR证据为准，
不复用此前a9744a0的成功、APK或SDK记录。

仍未完成：工作区冻结材料重启、安全启动通道及watchdog/CAS，工作区原操作和Packet关闭
屏障的进程持久恢复，远端原子前置条件、原生VPN/core装配、后台调度/插件/真实安装与
Galaxy S8/iPhone验收。A11及完整PRD不因本批通过而完成；四项许可材料限制继续保留。
契约：[工作区](../gui/config-workspace-local-v0.0.3.md)、[统计导航](../gui/endpoint-statistics-navigation-v0.0.3.md)、[Packet分页](../gui/packet-route-pagination-v0.0.3.md)。

### c422292 完整CI失败及修复

c422292的主CI真实执行608项：Chromium303通过/1失败，WebKit302通过/2失败，
总605通过/3失败；三项各重试一次仍失败，没有flaky或skip，非超时。其余13个主CI
job及另外三组工作流成功，仅作为该head历史证据，不能称本批最终全量通过。

修复共享settings fixture误用专项u64样例：保留原日志、客户端覆盖和最终inbounds三视图，
专项样例只供workspace panel。移动返回只把尚未关闭的dialog视为顶层；真实Portal已
closed但仍在退出动画时，不吞掉下一次浏览器/系统返回。原有返回详情断言保持。
新增两项显式保留2秒退出动画的回归；暂用旧逻辑时，两项都到达原详情返回断言并失败，
恢复修复逻辑后通过。最初仅改animation-duration的试验未保留Portal，停在前置条件，
不计为正反验证结果；使用显式keyframes后的日志才是有效证据。

修复后相关Chromium **75/75通过**，涵盖端点/共用统计、移动shell、旧端点/统计、
settings三视图和工作区；完整pnpm verify:frontend、生产build、类型0/0通过。
Rust源码及依赖未变，前述1181/0/7仍对应同一后端；新head CI另行核对。
本地WebKit官方下载返回403 Domain forbidden，未运行、未绕过；以同提交CI实际执行为准。
日志：/tmp/client050607-ci-fix-browser-verified.log、/tmp/client050607-ci-fix-frontend-final.log、
/tmp/client050607-closing-portal-negative-final.log。最终准确head、双引擎及新产物见同一PR。

## 原操作与 Packet 持久恢复（a51c6ce 后续）

本批在同一 Draft PR 增加普通工作区 schema4 原操作账本、持久关系 epoch 和随机
宿主会话。副作用前持久预留，只有原真实回执持久确认后才可明确检查并完成本地
原候选；SQLite 关系、AppConfig 确认和完成墓碑分阶段，不重放 apply/TUN/下载。
预览后的关系 A→B→A、宿主重建后的相同本地计数、旧票据及晚回执均不能取得旧执行
资格。AppConfig/规则文件仍只核当前值，不声称具有全部历史 epoch。

Packet 账本使用私有有界元资料、跨进程文件锁、原子持久预留和原 operation token。
读取列表缺行或已结束行不清账；只读核对原物理目标的权威结束事实或明确拒绝后，
还需清账持久确认。坏记录、丢失已初始化账本、写失败和晚 token 保留屏障。

审查中修复了 Complete 后内存屏障未退役、内存投影后可失败收尾漏配置失效通知、
正常 apply 换用新 binding、预检未固定持久 epoch 等边界。真实回执保存为历史事实
不因本地新 epoch 丢失；本地发布仍严格核对原 epoch。系统调时保留原始时间和标记，
一次性恢复票据按进程单调时间到期。冻结材料重启仍未实施，具体源码证据和安全
通道、实际依赖字节、路径、child/readiness/receipt/watchdog 条件已补入 todo。

最终本地 Rust workspace/all-features 测试 **1224 passed / 0 failed / 7 ignored**，
共45组，GUI lib为951/0/5。包括真实OS子进程重开、跨进程锁争用、SQLite回执与阶段
故障、实际文件权限拒绝、关系ABA、并发预留、票据过期及真实Unix IPC纯查询恢复。
专项工作区41/41、Packet37/37均通过；子进程子结果不再重复计入父harness总数。
7忽略项为外部Zero配置/观测、两代快照、两项真实发布内核readiness、live订阅provider
及Lifecycle文档测试；本地没有将它们计为执行。格式和全workspace/all-targets/
all-features check和同范围Clippy均通过；编译及Clippy警告保留，未按`-D warnings`
宣称零警告。准确提交CI以同一PR记录为准。

前端完整 `pnpm verify:frontend`、生产build和类型检查通过（**0 errors / 0 warnings**）。
Node工作区29+profile7=36、Packet26通过；320px实际Chromium旧14+新增9=
**23/23通过，0重试**。浏览器只模拟宿主事实重建，不能替代上述Rust真实进程测试，
也不能作为移动VPN验收。本地WebKit仍未安装，官方下载403没有绕过；待最终SHA CI
执行完整双引擎套件，不以a51c6ce的612例或旧产物代替。

早期专项实际出现18/8、34/5：测试候选错误省略真实合成、初始AppConfig未经过
生产规范化，以及重开测试遗漏私有目录；修复测试材料和路径，未放松校验。全量
首轮另有一个schema迁移断言仍期待3而实际为4；更新该版本断言并保留旧元资料检查后，
全集重跑通过。浏览器编辑期间的一次HMR重建失败在冻结所有相关源码后23例重跑通过，
未删除原分页重试断言或将其计为flaky通过。

日志：`/tmp/client-persist-workspace-test-third.log`、
`/tmp/client-persist-packet-tests-initial.log`、`/tmp/client-persist-final-tests-fixed.log`、
`/tmp/client-persist-final-{fmt,check,clippy}.log`、
`/tmp/client-persist-frontend-initial.log`、`/tmp/recovery-browser-freeze.log`。
本批没有新增依赖，四项许可材料缺口、原生装配、Galaxy S8/iPhone及签名/安装未验收项保留。

### 2eceeb4 的 Windows 订阅握手失败

该提交的 Windows latest 已实际通过客户端 **1154/0/5**，随后发布版内核 readiness
为 **1 passed / 1 failed**：IPC 健康检查完成后，新控制连接等待 subscribe ACK
5000ms 超时。Windows2022 同提交客户端与 readiness 合计 **1156/0/5**；对照通过
不能代替失败平台结果。Android unsigned host、Mobile Rust、Native document adapters
三组同提交工作流成功。主 CI 终态为13成功/1失败，双引擎 **630/630** 通过、
0重试/0flaky/0跳过；不能将这些成功部分称为该提交全绿。

只读核对实际发布的 core v0.0.1 `src/ipc/windows.rs`：同一 select 同时等待新管道连接
和旧连接 task 完成，旧 task 先结束可能使当前监听实例被丢弃。这是失败的候选原因，
未在此 Linux 环境声称复现 Windows 调度。客户端另有可确定的边界问题：reader
在 ACK 前退出时，Inner 仍持有 ACK sender，连接方因此等待整个超时才知道已断开。
本轮仅修复首次订阅、尚未交付业务请求的冷握手；已发送的 query/command 不因此
重放，也不重新发现 BoundControl 的目标。实际平台结果须继续核对修复提交的 CI。

readiness 占用端口测试同时加强：订阅握手失败本身必须使测试失败，不能当作实际
运行配置调用失败；应用失败回报原内核 stderr。没有放宽运行确认、API 身份或权限检查，
也未修改 core 源码。

修复只允许 reader 的实际断连信号在 ACK 前触发最多三次冷订阅；三次共用原截止时间，
每段订阅写入及 ACK 等待使用剩余预算，成功后恢复原业务写入预算。内核拒绝、坏 ACK、
解码错误、订阅写失败和 ACK 后断连均不在此重试边界。CONNECT_GATE、缓存发布及
BoundControl 的原物理连接保留。Unix 同步 connect、Windows 管道连接的既有底层等待
仍按 transport 实现，不声称本次增加了全部 OS syscall 的硬总截止。

修复后的实际 Unix IPC 控制连接专项 **14/14**，其中新增六项覆盖即时 EOF、第二次
ACK、最多三次、迟到 ACK 不刷新预算、拒绝/坏 ACK/坏帧和业务零重放。全 workspace
all-features 本地测试 **1230 passed / 0 failed / 7 ignored，45组**，GUI lib
**957/0/5**；忽略项范围与上一批一致。格式及 locked workspace/all-targets/
all-features check、同范围 Clippy 通过，保留现有警告。复核补回手动订阅写循环对
Interrupted 的继续语义后，最终格式/check/全集/Clippy 均重新通过上述结果。
前端源码未改，本地不重复执行刚完成的冻结前端套件；
修复提交仍需重新核对完整 CI，不将 2eceeb4 的 630 项或 APK 充作新 head 的产物。
日志：`/tmp/client-handshake-focused.log`、
`/tmp/client-handshake-final-{fmt,check,tests,clippy}.log`。


## 2026-10-15：文件历史与只读账本诊断

本批从全绿 `d8e6ac1272b02b4064044deb195206ac0e203950` 继续，在同一 Draft PR 中增加
AppConfig/规则正式持久写入的 OS 文件锁、随机存储身份、单调 epoch、原操作 intent/receipt
和主备字节核对；工作区、普通订阅及端点恢复固定原文件 tokens，拒绝合作 writer 的
同值保存与 A→B→A。schema5 保留旧数据；旧无 tokens 记录仅作历史，不补发控制许可。
原 owned 文件写已完成、检查点中断时，纯查看仅 probe，明确原操作恢复才可补确认
元资料；不重复写正文、不 apply、不下载。没有原 intent 或材料变化时保持未知。

工作区与 Packet 增加显式有界只读检查及固定错误分类，原草稿、文件和未知屏障保留。
工作区不读候选列，不进入正常创建/迁移/修复路径；SQLite READ_ONLY 仍可能写 SHM，
因此实际存在 WAL 时直接报告原因，无 WAL 时使用 immutable 只读主库并再次核对变化。
有真实临时库/WAL/SHM 保留与子进程检查测试，未操作用户数据。没有归档/删除/清账按钮。

冻结启动仍 unsupported；已逐项记录普通空 bootstrap、当前材料重读、Proxy 实际启动
顺序及公开快照缺失字段。PID/health 不确认 startup-only 字段；客户端安全材料/依赖封存/
watchdog 可独立继续，实际 Proxy startup acknowledgement 的最小协调范围见
[启动边界](../gui/frozen-startup-boundary-v0.0.3.md)，没有修改 core。

许可补入 svelte-toolbelt 0.10.6 精确包 LICENSE 的显式已审阅 MIT 映射，保留
manifest 未声明的事实及原两作者版权。其它三包仅补真实审计来源/限制；四项原缺口仍
保留，不用 sibling 版权或通用条款冒充该包材料，不声称最终链接材料完整。

本地 locked workspace/all-features 测试 **1292 passed / 0 failed / 7 ignored，45组**，
GUI lib **1019/0/5**。包括 file_history 10、SQLite只读检查15、Packet只读检查13、
端点文件历史10、普通订阅服务15与数据库17项；子进程输出不重复计入父 harness。
忽略项范围与前批一致，未当作执行。首轮两个失败分别是迁移 fixture 仍期待 schema4、
ABA fixture 捕获了规范化前 AppConfig；修正基准并保留旧数据/主备原字节及旧票据拒绝
断言后，完整重跑通过，没有放松生产校验。

完整 `pnpm verify:frontend`、生产 build、类型检查通过（0 errors / 0 warnings）。
新增只读诊断 Node16/16；冻结 Chromium 31/31 后 WAL 分类新增1例，最终该页9/9，
均0重试/0失败/0跳过；浏览器使用宿主事实 fixture，不代替真实进程或手机验收。
本地 WebKit 未运行，官方下载403没有绕过，最终双引擎结果以新提交CI为准。

日志：`/tmp/client-history-tests-{initial,final}.log`、
`/tmp/client-history-frontend-final.log`、`/tmp/storage-inspection-{node,check,browser}-wal.log`。
本批不改版本或 lockfile；只为已有 rusqlite 启用 hooks/limits 来限制只读 SQL 预算。
新 commit CI、无签名 APK 及 SDK 收据在同一 PR 中逐项记录，不借前批全绿替代。

最终源码又经 fmt、locked workspace/all-targets/all-features check、完整 tests 与同范围
Clippy 通过；测试仍为1292/0/7。Clippy 首轮拒绝两处权限测试的十进制零，修为八进制
`0o0`，同时简化两处新代码冗余引用/闭包；随后全集和检查重跑通过。现有及非拒绝级
警告保留，没有按 `-D warnings` 宣称零警告。最终日志：
`/tmp/client-history-tests-after-clippy.log`、`/tmp/client-history-fmt-final.log`、
`/tmp/client-history-check-final.log`、`/tmp/client-history-clippy-fixed.log`。

实际 Rust 许可 build-hook fixture 也通过：首次执行缺少云端 Cargo PATH 未启动检查，
补齐与正式构建相同工具链环境后成功，日志 `/tmp/client-history-license-build-hook-fixed.log`。


### 45c8838 的 Windows 路径分类 fixture 失败

该提交 Windows latest 和 Windows 2022 客户端各实际1210 passed / 1 failed / 5 ignored，
两者发布内核readiness步骤均未运行。失败来自新增Packet只读测试将普通文件作为父目录，却固定期待Linux的
`NotADirectory`→`io-unavailable`；Windows实际`NotFound`→`not-initialized`。
生产代码按操作系统实际错误分类，没有向UI回传路径或原始错误。修复仅调整测试按
平台核对真实错误种类、固定分类、canary隐私、原文件字节和目录条目保留，并显式
测试普通IO错误分类。不能用该提交的其它通过结果声称它全绿；修复commit重新验证。

修复后本地fmt、locked全workspace/all-target/all-feature check、完整tests及Clippy重新
通过，仍为1292 passed / 0 failed / 7 ignored（45组）。前端未变，不重复刚通过的
完整前端验证；新SHA仍重新观察全部CI。日志 `/tmp/client-history-windows-{fmt,tests,check,clippy}.log`。


## 2026-10-15：先审计剩余功能，再补普通配置文本编辑

基线 `19682c75dee28e3a11f45b805e5b25b0268f85c7`，仍为同一 Draft PR #46。
先记录[完整A01–A24完成度/剩余矩阵](../gui/mobile-client-completion-matrix-v0.0.3.md)，
区分已有客户端链、用户明确后置的原生装配、可独立继续的真实用户功能及外部条件。
未新增秘密启动临时文件、启动器或假Proxy加载回执；完整冻结启动仍有私有通道/
实际依赖/原child与watchdog工作，以及需要协调的实际加载绑定事实。

本批新增普通内嵌Zero来源的Rust文本快照、原生schema/引用校验及固定材料本地CAS；
JS不再将列表Value stringify或parse用户正文。原生校验返回原Value，保留u64及
被省略的默认字段，不表示逐字节保留JSON排版、修复历史舍入或通过编译协议/平台验收。
规则集/GeoIP在固定core的parse中可能读取文件，当前入口在此之前明确拒绝，未获授权
不读、不删字段、不伪称通过。managed/受保护/path-only来源不降级；普通订阅只读。
新建固定对象保持未启用；编辑保留当前选择、AppConfig及运行代理归属；保存为待应用，
没有IPC/TUN/运行事务，未知只读核对原固定目标，不重放。

本地locked全workspace/all-features测试 **1314 passed / 0 failed / 7 ignored，45组**；
GUI lib **1041/0/5**，包含原生文本校验10及本地保存12。覆盖实际SQLite重开、
最大u64、离线保存、零Unix控制端点连接、完整来源CAS/并发单成功、只读秘密边界、
实际FIFO在依赖读取前拒绝、文件原文字节及失败后原材料保留。仅Unix测试在Windows
不会编译为执行项，不能算Windows已覆盖。fmt、locked workspace/all-target/all-feature
check及同范围Clippy通过，现有非拒绝级警告保留，不称零警告。

首轮Rust测试fixture编译失败是socket字段应为Option<String>；随后原生聚焦9/10，
DoH域名服务器遗漏固定core要求的bootstrap。补合法文档测试IP后完整全集通过，
未放宽生产校验。首轮frontend静态测试仍将已移除的文件/JSON分段当消费者，更新
实际分段消费者集合；第二轮严格订阅harness发现配置服务转导出新增依赖，改为组件
直接调用独立feature client及专用浏览器alias，不放宽订阅测试或mock。

日志：`/tmp/profile-editor-{fmt-final,check,all-tests,clippy}.log`、
`/tmp/profile-editor-backend-second.log`、`/tmp/profile-editor-native-second.log`。
本地前端全集与双引擎/原生构建的最终SHA CI结果在同PR分别追加；任何未运行项
不借前批成功替代。不改依赖、lockfile或版本，四项许可缺口保持。

新编辑器Node **20/20**；最终Chromium聚焦 **17/17**、0失败/重试/跳过，
含新编辑器8、原生文档原4、原profiles4、mobile配置1。首轮17中15通过，2项是
测试对同名关闭按钮的严格定位冲突、试图点击真实禁用关闭；改为明确footer定位并
通过真实导航guard验证busy阻止离开，未删断言/测试。最终专用fixture完整字段与
feature client alias均在本次17项执行中使用。完整类型检查0 errors/0 warnings，
生产构建通过；类型检查曾发现fixture缺领域必填字段及旧switch CSS，已修正。
日志：`/tmp/profile-editor-node.log`、`/tmp/profile-editor-browser-typedfinal.log`、
`/tmp/profile-editor-check-frontend-fixed.log`、`/tmp/profile-editor-build-final.log`。
本地WebKit未运行；Chromium宿主事实fixture不等于系统文件提供者、SQLite或真机验收。

最终源码的完整 `pnpm verify:frontend` 已从头重跑通过，日志
`/tmp/profile-editor-frontend-verified.log`，含全部功能回归、新Node20、许可检查、
0错误/0警告类型检查及生产构建。没有将前轮失败命令记为通过。

### 未确认保存的关闭标题修正

`001da17c744d49c885898ea3df27ac3cbba90e8e` 四组CI实际全部success/attempt1，
主CI14/14、双引擎664/664（每引擎332，新增编辑器各8），无失败/重试/波动/跳过。
等待时发现：unknown关闭确认正文已说明原保存可能已完成，但标题仍称“未保存”。
仅为unknown改为“关闭结果未确认的编辑器？”，普通dirty与saving标题保持；现有
浏览器用例补正确标题及旧标题不存在断言，不增加恢复链。修正后完整前端验证重新
通过、类型检查0/0、build通过；同17 Chromium再次17/17（0失败/重试/跳过）。
日志`/tmp/profile-editor-heading-frontend.log`、`/tmp/profile-editor-browser-headingfinal.log`。
新SHA的CI仍须重新观察，001da结果仅历史，不借作标题修正后的准确提交绿灯。

## 2026-10-15：A18 Flow 固定选择与 A19 真实诊断

基线 `66be1d154860ee55403d5bc3a5a5399ed1a907ad`，沿用 Draft PR #46→develop；
不改 core、不重做原生 VPN/宿主装配、不创建签名或发布。A04 已审计，当前入口定位和
规则模式出口事实仍有完整导航/dirty guard 工作，本批继续明确保留缺口。

A18 增加宿主真实 Flow 清单、显式多选、固定预览、单次提交与逐项六态。旧活动行或
暂停快照不自动勾选新清单；旧 GUI 关闭兜底与 fulfilled 计成功已移除。闭合到同一
物理连接、真实运行实例/revision、观察租约和客户端代次。closed:false、缺字段、
not_found 或缺行均不算关闭成功；未知只读核对原操作，不换计划重放。
现有 core wire 仅有 flow_id，无远端原子实例/revision 前置条件；当前宿主记录只在
会话内，明确不称跨进程持久恢复。移动实际控制桥未接入时 unsupported。

A19 路由工具消费实际 CommandResponse，展示模式、匹配规则与路由动作，删除伪逐跳
TTL/RTT；索引及 DNS 缓存 TTL 保留十进制字符串。独立 dns_cache 入口复用共享任务，
未启用、未命中、有效空列表、失败和不完整响应分别呈现，不制造查询来填缓存。

本地新增后端专项：Flow实际Unix IPC 11/11、严格legacy回执1/1、路由5/5与DNS缓存3/3；
观察租约4/4。fmt、locked workspace/all-targets/all-features check已通过。类型检查
0 errors / 0 warnings。上述专项不代替完整测试或最终SHA CI，后续准确结果在下方追加。
日志：`/tmp/a18-flow-close-rust.log`、`/tmp/a18-legacy-close-rust.log`、
`/tmp/a19-{route,dns-cache}-rust.log`、`/tmp/flow-diagnostics-{subscription,fmt,check,typecheck}.log`。

最终本地完整Rust全集 **1335 passed / 0 failed / 7 ignored，45组**，GUI lib
**1061/0/5**；fmt、locked workspace/all-targets/all-features check及同范围Clippy
通过，保留现有非拒绝级warning，不称零warning。完整 `pnpm verify:frontend`
最终从头通过，含新增Flow Node20/20、路由/缓存Node9/9、原tool scope26/26、
0错误/0警告类型检查及生产build。Chromium：Flow新11+旧history1 **12/12**；
A19新6+旧tool scope4 **10/10**，均无重试或跳过。本地WebKit缺binary未运行，
不能以Chromium或旧提交CI代替本批双引擎验证。

首轮Flow浏览器因新增spec参数语法错误未执行测试，同轮前端全集在类型检查失败。
修正spec后第二轮旧history1通过、新11失败，发现session相对client导入绕过专用fixture
alias，误调无Tauri的真实invoke；改为标准$lib feature别名，业务逻辑不变，Node20与
原12浏览器重跑通过。修正后的完整前端全集再次通过，没有删用例或弱化关闭断言。
日志：`/tmp/flow-diagnostics-{all-tests,clippy,frontend-final}.log`、
`/tmp/flow-close-node-final.log`、`/tmp/flow-close-browser-alias.log`、
`/tmp/a19-route-cache-browser-final`。新SHA全部CI仍须独立观察，历史66be结果不借用。

## 2026-10-15：A10 配置路由规则覆盖纵向功能

基线 `9b1e1a57c39c184e7a8eaf0080b307001131e202`，同一 Draft PR #46→develop。
固定core schema只有condition/action/mode；本批不增加原生enabled/id/protocol条件。
按用户允许的完整纵向范围，先交付普通来源规则CRUD、排序、启停及恢复来源；规则集
引用迁移和关联A13默认DNS删除迁移明确留在矩阵，未沿旧删除/普通重启路径假报完成。

`routeRules`只存AppConfig.profileEdits[profileId]。Rust保留完整原始source及逐行原生
JSON文本；停用行仍在客户端保存并校验，仅启用rule进入合成。完整原source摘要变化
后旧覆盖只作历史，须显式重建或恢复；普通订阅不改来源，托管/保护/非Zero或文件依赖
来源不开放该入口。规则集/GeoIP在原生parse前拒绝，未新增文件授权resolver。

宿主5RPC绑定完整来源/图/关系epoch、AppConfig与规则文件history、host session和
scope generation；固定整组预览及单次本地保存。真实SQLite IMMEDIATE栅栏在AppConfig
发布期间锁住协作关系写者，不初始化/迁移缺失或外来数据库，不在文件成功后追加可失败
DB commit。成功视图核对原owned写入after token及自身代次，不借后来写者快照冒原回执。
未知票据仅在会话RAM保留且不重放；不称跨进程计划恢复。保存后仍须显式工作区预检/应用。

中央合成重新核来源归属，先profile投影、后公共规则；legacy DNS预览避免再次替换已
合成规则。组合报告分开两层。旧profile_settings_apply的routeRules写入/恢复在读取和
副作用前拒绝，不能绕过固定预览或触发运行。原设置导入白名单不扩到profile材料。

前端最终独立Node **23/23**，实际MobileShell/配置入口Chromium **12/12**，含320px、
44px、4底栏、CRUD/排序/启停、旧规则集、dirty/busy、来源重建、迟到及丢回执核对。
最终完整 `pnpm verify:frontend`通过，类型检查0 errors/0 warnings，生产build通过。
本地WebKit缺binary未运行，等待本提交CI，不借旧SHA结果。

首轮移动fixture误接占位配置snippet，前4例失败后停止该测试进程；换专属HTML并保留
真实组件/断言，依次10/10、11/11、最终12/12通过。Node/read-only复核修正了自身保存
改变binding被误判外部漂移：原status确认saved后，必须与新鲜当前snapshot binding一致
才交付本地成功，读失败/真实漂移仅历史，未重放保存。

首轮Rust生产check通过；最初owner10通过，Root组合fixture未显式保存实际公共规则
偏好导致1例失败，补真实client setting后组合通过。严格交付后核又拒一例手改来源但
未通知scope的fixture；仅修测试模拟真实来源writer并断言revision/gen推进，另加保留
未通知缓存必须unknown的反例。生产guard未放宽。SQLite graph栅栏专项4/4已执行。
最终完整Rust及准确SHA CI结果在下方与PR追加。

为容纳新双引擎覆盖，CI将Chromium/WebKit拆成并行任务，各自完整测试与独立产物；
保留原“UI controls (Chromium and WebKit)”检查名作为两者必须成功的汇总门，不改分支保护。

日志：`/tmp/route-editor-{check-initial,graph-fence,backend-initial,backend-final,backend-refrozen,frontend-final}.log`、
`/tmp/route-editor-node-receipt-final.log`、`/tmp/route-editor-browser-receipt-final.log`。

最终本地Rust完整全集 **1368 passed / 0 failed / 7 ignored，45组**，GUI lib
**1094/0/5**。新宿主14+中央组合3专项17/17、纯投影11/11、SQLite图栅栏4/4及旧RPC
旁路拒绝测试均执行；包含真实AppConfig文件重开、SQLite source/u64保留、关系/文件ABA、
实际写锁、FIFO零读取、未知不重放及后继写者不能冒原回执。fmt与locked全workspace/
all-targets/all-features check、同范围Clippy通过；现有非拒绝级warning保留，不称零warning。

最终完整前端全集重新通过，含Node23/23、0错误/0警告类型检查及生产build。
Chromium最终12/12，0失败/重试/跳过；本地WebKit未运行，双引擎以新SHA CI为准。
最后的source writer fixture修复后，另一个未通知缓存反例也按预期unknown，无放宽生产准入。
日志：`/tmp/route-editor-{all-tests,check-final,clippy,fmt-final,frontend-final}.log`、
`/tmp/route-editor-backend-refrozen.log`、`/tmp/route-editor-projection.log`。
CI YAML已解析核对；并行engine任务各有独立报告，原名汇总检查只在两者成功时通过。


## 2026-10-15：规则集引用与 DNS 删除迁移

本批基线64d1f53，同一Draft PR46，先核源码引用/所有权及真正本地提交边界。
新增[迁移契约](../gui/reference-migrations-v0.0.3.md)，不把GUI id、原生tag、同名或path作为相同资源。
规则集扫描来源规则/旁路/DNS、所有覆盖行（含disabled）、全局及保留DNS；保护/托管/无法检查及不安全跨配置范围明确阻断，原文不改。
当前普通配置以sourceDigest绑定的ruleSetReferences纯投影迁移；AppConfig先发布引用，再删除规则集记录，明确非跨文件原子。
两份owned回执与严格后置状态确认成功；阶段失败保留原操作/阶段事实，不盲目回滚或重放，票据仅宿主会话内有界保留。
替代产物验证精确私有owner目录、普通文件、64MiB预算、完整ZRS版本/大小/checksum/entrycount及实际SHA256；FIFO/链接/外部路径拒绝。

实际消费链也已验证：common定义注入后再核全部路由/旁路/DNS引用，重组合只移除匹配的最后生成行，保留显式source规则。
工作区只对带原合法迁移覆盖且Value完全未变的source，先typed schema/duplicate检查、组合后完整nativeparse；来源变化不能借此读新依赖。
测试使用实际encoded ZRS和工作区from_input完成真实最终原生编译，不以pure helper通过冒充应用链可用。

DNS删除迁移从host保存材料取原值，只接明确服务器/替代intent，列全default/dispatch/node/direct/三回退链/独立timeout键。
超时键碰撞、重复链/主备冲突与不能核对的extra不隐式归并；禁用DNS仍核保留配置。
保存全局客户端覆盖待应用，明确影响全部配置；不验证其它保护来源的运行兼容性。
旧shortcut server删除/rename在effects/落盘前拒绝；本批专用流只有delete+migrate，rename固定预览未适配并提前提示保留草稿。

迁移实现本地完整locked Rust workspace/all-features：**1431 passed / 0 failed / 7 ignored，45组**，GUI lib **1157/0/5**。
其中新DNS18、规则集owner15、纯引用20、真实artifact7，以及中央组合/完整引用/保留source规则3项，共新增63项。
完整全集先通过；随后收口新增publish参数数量Clippy警告为固定LocalCandidate，并补legacy新字段及原文重复键断言后，再次完整1431/0/7通过。
不将ignored称作已运行。格式、全目标workspace check及Clippy以最终执行日志/准确head PR回执为准，既有警告不隐藏。

完整pnpm verify:frontend通过，类型检查0错误/0警告及生产build通过。
新增实际production state/client Node：DNS21 + rules23 = **44/44**；最终同源Chromium新两套 **23/23，0失败/重试/跳过**。
DNS原3项另原断言回归通过；本地WebKit未安装，不绕过或下载，最终双引擎以新SHA CI真实执行为准。
本地日志为/tmp/reference-migration-{all-tests-final,frontend-final,browser-final,check-refrozen,clippy-refrozen}.log。

失败过程保留：首轮all-target编译时pure测试文件尚未落，production lib已过而test编译停止；之后配置namespace187/0/1实际通过。
首轮typecheck发现onretry Promise<boolean>不匹配，已修并完整0/0通过。
DNS首轮存在未完成并行fixture alias、同名关闭locator以及真实busy Escape可关Bits内部modal问题，修复后新11+旧3=14/14；未放宽旧断言。
Rules浏览器发现非rune writable不更新及无当前profile刷新仍留旧选择，已修为view派生和明确失效，最终12/12。
Root第一次合并browser命令缺PLAYWRIGHT_REUSE_SERVER开关，启动前报4177占用、未跑用例；保持现server，正确参数后实际23/23。

本批无core/dependency/lockfile/version修改，不用用户Mac，不创建签名凭据、不合并发布。
原生VPN/core/宿主装配及Galaxy S8/iPhone真机验收仍后置；四项许可材料缺口保持。
跨配置或保护来源迁移、完整rename/其余A10/A13条件能力不因本批通过而完成。
最终提交四组CI及七份artifact按实际新head记录在同一Draft PR，不借64d1或更早head的成功。

### 同批 CI 发现的下载锁释放修复

552e29e 的 Main CI #202 / run38753766515 中，macOS 下载验证真实执行 19 项，18 通过、1 失败：
`complete_on_disk_without_marker_handles_416_then_still_requires_caller_validation` 在 drop(Cache) 后重开返回“正在下载或校验”。
后续五项 kernel/license 验证被跳过，该 run 不能作为最终绿色收据。
Cache 只靠 File close 释放锁；[Rust 文件锁契约](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
允许重复或继承描述符延长锁寿命。并发 spawn 是可能的持有者，CI 日志本身不证明实际发生了哪次 fork。
新增 Unix 确定性回归保留 try_clone 描述符，旧实现真实复现同一 busy 拒绝（0 通过、1 失败）。
修复在 Cache 唯一合法 owner Drop 时显式 unlock，Download 仍持有 Cache 到校验/消费完成；不删锁 inode、不添加等待或重试。
回归同时验证 owner 活着时拒绝独立打开，owner 结束后副本仍活也可重开，以及旧副本关闭不能释放新 owner 的锁。
修复后重新执行完整 locked workspace/all-features：**1432 passed / 0 failed / 7 ignored，45组**，GUI **1158/0/5**。
新增描述符回归及原416用例均实际通过；日志 `/tmp/reference-migration-cache-fixed-all-tests.log`。
修复后 fmt --check、locked workspace/all-targets/all-features check 与 Clippy 均通过，既有警告保留；
后两项日志为 `/tmp/reference-migration-cache-fixed-{check,clippy}.log`。
最终新 head CI 按实际结果记录于 PR；552e29e 的产物只作历史诊断。

### 同批 WebKit 测试对话框定位修复

451c372 的 Chromium 实际 384 通过；WebKit 实际 383 直接通过、1 项重试后通过，不能记为零重试。
首次失败为 `mobile-route-editor.spec.ts:13` 的 locator strict violation：全局“继续编辑”同时匹配 busy guard 与正在退场的预览 Portal。
“设置正在提交”已可见，日志没有保存状态或返回行为断言失败；这次证据只支持定位歧义，不称产品 busy guard 失效。
仅将该点击与既有可见断言限定到标题精确为“设置正在提交”的 dialog；完整 save-once、草稿、返回及 saved_pending_apply 断言保留。
不增加 sleep、retry、timeout，不改变产品逻辑。正确复用4177的本地 Chromium 定点真实 1/1 通过（0失败/重试/跳过），
日志 `/tmp/reference-migration-dialog-locator-chromium.log`。本地无 WebKit，最终双引擎按新 head CI 记录，不借451的重试结果。

## 2026-10-15：概览对象定位与有限运行网络

本批基线 `90557080d337258f956dea68a22ba8323b43e43c`，继续同一 Draft PR #46 → develop。
契约见[概览定位与移动运行网络](../gui/mobile-location-network-v0.0.3.md)。
A04新增只读公开目录binding及真实NodeScreen选择链，不读取正文或SQLite，不授予控制权限。
跨底栏guarded visit保留原搜索、过滤、滚动、订阅分段、嵌套栈及草稿；返回精确来源。
规则/直连/运行未确认分别表达，不借历史globalOutbound制造唯一叶子；同名、旧实例及导航ABA不重定位或执行。

A12新增普通内嵌Zero上下文中的四字段窄预览/本地CAS：MTU、完整u64 UDP超时、既有监听IP和端口。
TUN复用统一只读MTU，非回环LAN范围固定确认；没有监听不新建。Direct leaf地址族/物理出口保留真实只读值与工作区入口。
旧未规范化覆盖、托管/保护/path-only或混合身份明确只读，不在网络保存中暗迁其它字段。
来源schema只做typed投影，不打开规则集或GeoIP依赖。原owned文件回执、SQLite关系epoch与scope/session共同核对，保存仍待应用，无IPC/TUN/进程/系统代理操作。
返回/前台只读刷新保留同binding脏草稿；变化后只读，明确放弃后才采用新来源。error禁编辑/预览，busy/unknown不替换原操作。

完整locked workspace/all-features Rust：**1457 passed / 0 failed / 7 ignored，45组**；GUI **1183/0/5**。
新增A04目录8及A12网络17共25项；包括实际磁盘/SQLite重开、零IPC、严格字段/u64、原文保留、ABA、未知及后来写入者。
全目标workspace/all-feature check与fmt通过。完整Clippy按仓库标准通过，既有警告保留；额外尝试 `-D warnings` 因既有native-io/plugin及其它警告失败，不当成绿色证据。
本批新增网络测试的default字段初始化警告已修，原17项再次实际通过，不压制lint或改变断言。
完整 `pnpm verify:frontend` 通过，Svelte **0错误/0警告**及生产build通过。
最终新增production Node **54/54**（导航13、身份19、网络22），0失败/跳过。
实际相关Chromium **69/69**（新定位20/旧20，加新网络14/原设置15），0失败/重试/flaky/跳过。
日志 `/tmp/mobile-location-browser-unconfirmed-final.log`、`/tmp/runtime-network-browser-resume.log`；fixture只提供明确宿主事实，不作设备成功证据。
本地WebKit未安装，不下载或绕过；最终双引擎和新提交四组CI/七份artifact按实际head记录于同一PR。

失败过程保留：初始Rust test compile误用另一owner私有test hook，修正为本域owner；其后1项原生schema fixture错误，修正为真实schema，没有放宽parser。
legacy检查的MutexGuard引用类型错误修正后全目标通过。首轮前端声明次序及非交互tabindex产生2错误/1警告，修正为实际行按钮focus，最终0/0。
A04早期失败包括配置按钮正则同时命中出口说明、离线过滤fixture错误使用会被保留的已选对象；限定实际按钮并采用真实未选离线节点，业务断言保持。
实际概览异步定位ABA回归先失败，再补离开取消代次，同一反例及后续完整相关回归通过。
A12旧三项网络草稿fixture误称存在监听，改为无监听来源事实；原草稿、返回、busy和保存断言保留通过。
本地日志：`/tmp/mobile-location-network-{tests-candidate,network-tests-refined,check-final,fmt-refined,clippy-repo,clippy-refined,frontend-final,node-final}.log`。

DNS rename、跨配置/托管引用迁移及完整网络条件编辑仍分别待办；不把本批称作整个A04/A12或移动端完成。
原生VPN/core/宿主装配与Galaxy S8/iPhone验收继续后置，四项Android许可材料缺口保留。
没有core、依赖、锁文件或版本变更，没有使用用户Mac、创建签名凭据、合并、发布或修改分支保护。


## 2026-10-15：A23 版本目录前台到期执行

本批基线 `6b4bdb46e06d7cc422d72141799ffe7525f4d249`，继续同一 Draft PR #46 → develop。
契约见[移动版本目录的前台到期检查](../gui/mobile-release-foreground-v0.0.3.md)。
原生宿主根据已保存 clientAutoCheck/clientChannel 和主窗口前台生命周期执行只读目录检查；
持续前台也能跨到期点执行，成功六小时、失败十五分钟，手动与自动共用 single-flight。
这不是 A09 订阅同步，也不复用业务写 UNKNOWN 的自动重放；不安装、下载包或修改运行配置。
200 完整校验后发布缓存；304 需要实际发送的有效 validator 与同源已验证正文。
偏好保存使用固定操作 ID、真实 owned 文件历史和持久回执；丢回复只核对原操作，不能以当前值相同补造确认。
确认重新核对文件历史、作用域、宿主和前台代次；损坏记录或确认落盘失败暂停自动检查。
原生事件仅唤醒 UI 只读刷新，页面挂载和可见性不发起目录 GET；脏草稿/未知操作保留，明确放弃才退休旧读并重新载入。

最终完整 locked workspace/all-features Rust：**1477 passed / 0 failed / 7 ignored，45组**；GUI **1203/0/5**。
新增调度 owner 20 项，包括真实 HTTP 200→304、有限正文、single-flight 等待者、晚到结果失效、
真实磁盘/SQLite 重开与故障触发器、原回执、同值 ABA、跨宿主重新审阅和前后台 ABA。
fmt --check、locked workspace/all-targets/all-features check 与完整 Clippy 均通过，既有警告保留。
日志 `/tmp/a23-release-{fmt,check,tests,clippy}-refined.log`。
完整 `pnpm verify:frontend` 通过：Svelte **0错误/0警告**和生产 build 通过；
日志 `/tmp/a23-release-frontend-refined.log`。
发行专项 Node **27/27**、Chromium **12/12**，0失败/重试/flaky/跳过；原7项Node及3项浏览器覆盖保留。
日志 `/tmp/a23-mobile-releases-node-final-27.log`、`/tmp/a23-versions-browser-final-27.log`。
本地 WebKit 未运行，不安装或绕过；最终双引擎、四组 CI 和七份 artifact 以实际最终 head 记录在同一 PR。

失败过程保留：首次浏览器 8通过/2失败，fixture 错用 plugin:event 而真实事件 alias 是 window CustomEvent；
修正专用 fixture，没有改变生产事件边界或放宽断言。随后明确放弃/重新读取的浏览器反例真实失败，
补齐 active/visibility 只读恢复和显式放弃；Node 的在途只读 loading 放弃反例也真实失败，
修复为退休旧读取代次，saving/unknown/confirming/checking 仍拒绝放弃。
实际 Rust 前台 ABA 反例在旧实现 **0通过/1失败**，原确认错误接受 leave→enter；
补核对生命周期后同一反例及最终完整1477项通过。日志 `/tmp/a23-release-lifecycle-red.log`、
`/tmp/a23-versions-{browser-initial,discard-red}.log`、`/tmp/a23-mobile-releases-node-loading-discard-red.log`。
先前1476项绿色在该修复前，只作历史，不替代最终1477项。

A22 仅只读审计，具体资源预览、明确替代/停止、跨存储逐项剩余和重开恢复仍是独立待办，未实际卸载用户资源。
OS 后台调度、安装器/商店渠道、签名安装、原生 VPN/core/宿主装配和 Galaxy S8/iPhone 真机均未验收；
不能把本批称作整个 A23 或移动端完成。四项 Android 许可材料缺口保留。
本批无 core、依赖、锁文件、版本或 CI YAML 变更，不使用用户 Mac，不创建签名凭据、合并、发布或修改分支保护。

## 2026-10-15：A22 固定归属卸载与持久逐项回执

本批基线 `c216a77ce9b19a31c9f6633b14f76ee66cd38213`，继续同一 Draft PR #46 → develop。
契约见[插件卸载的固定预览与逐项结果](../gui/mobile-plugin-uninstall-v0.0.3.md)。
预览固定实际安装材料、发布者、原 namespace、托管订阅/配置/规则及引用；确认重新核对版本和历史。
同名、ID 前缀、插件路径与后来新增资源均不是归属。活动配置、受保护/共享/无法检查的关系明确阻止，
展示手动配置/断开路径；不隐式切换、停止或取首个剩余配置。仅可证明安全的非活动范围进入本地执行。
持久原操作逐项记录完成、残留、未知和手动状态；独立最近记录不依赖包目录存在。
只读核对读取原持久回执，不重新扫描当前物理资源、不推进清理，也不自动重放未知操作。
合作文件历史、SQLite 关系 epoch、原安装代次与跨宿主活动租约拒绝后来写入者及旧回调；
这是分阶段多存储操作，不宣称跨文件原子事务或对恶意整体回滚的保护。
包 blob、已编译 ZRS 和共享 vault.key 刻意保留并说明，不宣称清空全部物理材料。

最终完整 locked workspace/all-features Rust：**1515 passed / 0 failed / 7 ignored，45 组**；
GUI **1241/0/5**。新增卸载域 39 项源码用例（26 owner、13 collector），包含实际磁盘/SQLite 重开、
子进程写入与未知操作恢复、固定归属/全部引用、同值 ABA、跨宿主迟到回调、同包重装、
祖先链接替换、伪造回执、UTF-8 预算、锁争用，以及真实签名但无运行授权包的登记删除。
缺少运行授权不制造组件删除项；存在的退役/错材料授权仍阻止本入口。
fmt --check、locked workspace/all-targets/all-features check 与完整 Clippy 均通过，既有警告保留。
日志 `/tmp/a22-rust-{fmt,check,tests,clippy}-final.log`。

最终完整 `pnpm verify:frontend` 通过，Svelte **0 错误/0 警告**及生产 build 通过；
日志 `/tmp/a22-frontend-precommit-final.log`。新增 production Node **16/16**，
新增实际 Chromium **14/14**，包含 320px 浅深色大字号与 44px 控件；原插件 Chromium **22/22** 保留通过。
legacy wrapper 清理后原完整流程再次定点 **1/1** 通过。浏览器均 0 失败/重试/flaky/跳过。
日志 `/tmp/a22-plugin-uninstall-node-final-16.log`、`/tmp/a22-uninstall-browser-complete-final.log`、
`/tmp/a22-plugins-existing-browser.log`、`/tmp/a22-plugin-uninstall-legacy-browser-final.log`。
本地未运行 WebKit，不安装或绕过；最终提交的双引擎、四组 CI 和七份 artifact 以实际 head 记录于 PR。

失败过程保留：初始 Rust check 的私有可见性错误修正为插件域内部权限；首次 focused
16 通过/17 失败均在 fixture 材料非 Base64 的构造处，修正 fixture 而不放宽校验。
随后 33 通过/2 失败为冷宿主未初始化目录的 fixture，改用真实 snapshot 恢复并保留 checked/component 断言。
完整插件域首次 88 通过/1 失败发现实际授权退役顺序回归，原授权撤销断言保留。
随后 89 通过/1 失败为敏感标识 canary 缺少真实待删规则；补齐严格归属及持久材料后，
完整候选 1514 通过/0 失败/7 忽略，仍不替代后续修复未获运行授权包卸载后的最终结果。
前端首次 Node 14 通过/1 失败修复明确拒绝后的 pending 槽；浏览器确认勾选后按钮响应失败，
改为实际响应式依赖。旧 wrapper 拒绝卸载后仍触发 TUN/配置副作用的反例真实失败，改为纯 invoke 后通过。
路径祖先替换、伪造回执及跨宿主迟到写入由源码审计发现并补回归；没有执行旧实现反例的不称 red 证据。

所有删除测试使用隔离 fixture，未实际卸载用户资源。完整活动配置运行替代/停止、移动插件容器与 SDK、
原生 VPN/core/宿主装配、签名发行及 Galaxy S8/iPhone 真机仍未验收，不能称整个 A22 或移动端完成。
四项 Android 许可材料缺口保留；无 core、依赖、锁文件、版本或 CI YAML 变更，
不使用用户 Mac、不购买账号或创建签名凭据、不合并、发布或修改分支保护。

### 同批许可测试的宿主依赖解析修复

中间提交 `0ed6bd983b9e749a48f7d93c696037e796a5bcae` 的 Main #207（run38794587437）
Ubuntu下载 job116627516485 在实际许可 build-hook 测试失败。此前五组 Rust 契约合计
57 通过/0 失败/3 忽略；失败是未过滤目标的 `cargo metadata --locked --offline`
要求缓存未编译目标的 `android_system_properties v0.1.5`，不能记为全绿。
macOS 同批 job116627516536 的相同许可步骤随后也真实失败，首错同为缺少该 crate；
两平台此前的五组 Rust 契约各 57 通过/0 失败/3 忽略，不抵销最终失败。

在 `/tmp` 独立 Cargo cache fixture 仅省略该 crate 的 source/archive（原缓存不改），
原完整脚本真实退出 1 并复现相同离线错误。修复为 `rustc -vV` 的真实宿主
`--filter-platform` 元数据，只用于已构建 build-dependency rlib 身份；保留 locked/offline。
同一冷缓存及普通缓存下完整脚本均通过；Android库存匹配、目标/feature/input SHA/正文篡改、
缺失材料及诚实 debug stub 原断言全部保留。日志 `/tmp/a22-license-{cold-red,cold-green,host-green}.log`。
没有修改 CI YAML、依赖、锁文件或许可材料，不消除四项 Android 许可缺口。
中间 head 的结果只作失败历史；最终 head 的四组 CI 与七份产物重新核对于同一 PR。

### 同批 Windows 安装锁争用分类修复

中间提交 `61f26723d20df17083082fbef36360fba566060d` 的 Main #208（run38796035683）
Ubuntu/macOS 已通过上述许可 build-hook；Windows2022 job116633419200 与
Windows2025 job116633419280 均在真实安装锁争用测试失败，各 GUI 1144 通过/1 失败/4 忽略。
失败断言是持锁拒绝至少等待原五秒截止；此前拒绝本身已通过，后续释放后成功断言尚未执行。

依据实际 fs2 源码，Windows 的锁冲突为 `ERROR_LOCK_VIOLATION`，不必映射为
`ErrorKind::WouldBlock`。Store 原循环将其当成普通 IO 错误立即拒绝；现同时识别
`fs2::lock_contended_error()` 的精确平台错误码，保持原五秒截止，其他 IO 错误仍立即失败。
不删除或放宽原等待、八秒上界、材料不变及释放后成功断言，不增加重试或跳过 Windows。
本地 Linux 不替代 Windows 修复后验收；新 head 的两实际 Windows 镜像重新验证。
修复后完整 locked workspace/all-features 再次 **1515/0/7，45组**（GUI1241/0/5），
fmt、全目标全特性 check 与完整 Clippy 通过，既有警告保留；
日志 `/tmp/a22-lock-{fmt,check,tests,clippy}-final.log`。

### 同批 WebKit 关闭过渡期定位修复

中间提交 `8b58b7d1bfe9c80a1a89d453c4f98286b9a2d752` 的 Main #209（run38798759888）
Chromium 完整 441 通过；WebKit 440 通过/1 失败，失败项重试一次仍失败，0 flaky/skip，
双引擎门禁因此失败。原锁争用用例已在两个 Windows 镜像实际通过，各工作区1412/0/5，
随后各2项真实 released-kernel readiness 通过；不以这些通过抵销 WebKit 失败。

失败是新增 `mobile-plugin-uninstall.spec.ts:75` 的全局“卸载记录”定位同时匹配
已返回的页面入口及尚在关闭过渡期的旧面板按钮。日志未报告原操作编号、只读状态查询
或不重复卸载的业务断言失败。helper 在明确关闭后等待原卸载及关闭确认 dialog 实际退役，
再执行既有入口与全部业务断言，不修改产品逻辑、不 `.first()`、不增加 sleep/retry/timeout。
本地无 WebKit，最终修复由新 head 的完整双引擎 CI 验证；旧 head 收据只作历史。
修复后本地实际 A22 Chromium **14/14**，0失败/retry/flaky/skip；
日志 `/tmp/a22-uninstall-browser-close-retirement.log`。4177 未重启，业务源及原全部业务断言不变。

## 2026-10-16：DNS 重命名固定预览与本地引用迁移

用户明确授权从 `a7ebb36b815be8dfb9deffb49199e119039b8500` 继续云端开发并提交同一 PR #46。
开工工作树干净，分支仍为 `feat/mobile-v0.0.3`，远端 develop 为 `fd198ba52711f018334860b8a6704546ccd349fd`。
检查时发现原 PR 被转为 Ready；按本批 Draft 要求使用正式 convert-to-draft 接口恢复并核实成功，未新建 PR。

本批复用 DNS 删除的宿主快照、固定计划、CAS 与 owned receipt，新增窄 `renameTo` 意图。
明确选原服务器并输入新名称，预览完整真实引用，确认仅提交 planId；Rust 移动原定义 Value，
只替换 default/dispatch/node/direct/三回退链/timeout 键，u64、顺序及未呈现的有效原生字段保留。
不改 host/domain/bootstrap/TLS 名称/detour，不接收页面 DNS 正文，不自动合并同名服务器。
原对象与候选均完整校验，禁用 DNS 也不跳过。取消不保存，过期、文件/图 ABA、scope 和所有权变化拒绝旧许可。
保存只为 `saved_pending_apply`，不发内核运行命令，不称平台 DNS/VPN 已应用或跨文件原子事务。

来源归属审查发现：原始 source DNS 若已有 ruleSetReferences 投影，再晋升全局会带回旧标签。
无显式全局且多配置、或来源含此投影时明确返回 scope 拒绝；带真正全局 DNS 的对象可封闭迁移，
不改变其它托管正文或 SQLite 来源关系。真实有效投影的正反例证明已迁移 tag 不回退，
受限范围列入 todo 与完成矩阵。未知只读原 plan/status，原实际成功回执仍须当前 binding 相符；
不自动重放、不换操作号，不提供跨宿主/进程重建保证。

本地前端最终完整 `pnpm verify:frontend` 通过：19 个有数值汇总的 Node 组 **672/672，0失败/跳过**，
其它合同脚本通过，Svelte **0错误/0警告**、生产 build 通过；日志 `/tmp/dns-rename-frontend-final.log`。
DNS production state Node **33/33**（原删除21+新增12），日志 `/tmp/dns-rename-node-final.log`。
实际同源 Chromium **24/24**（原删除11+新增重命名13），0失败/retry/flaky/skip，
覆盖320px/大字号/44px控件、全部引用、取消、忙碌返回、冲突/过期、托管限制、原未知操作及面板重建；
日志 `/tmp/dns-rename-browser-remount-final.log`。4177 未重启；本地无 WebKit，不安装或绕过，
双浏览器及四工作流/七份产物以最终提交真实 CI 与上传收据记录于同一 PR。

失败与修复保留：

- 新增晚到只读 status 拒绝反例首轮 **31通过/1失败**，配置变代次后错误回到 choosing；
  补原 epoch/conflict fence，含 status 和后续 snapshot 读取中变化的最终33项全部通过。
  日志 `/tmp/dns-rename-node-red.log`、`/tmp/dns-rename-node-final.log`。
- unknown rename 关闭 DNS、回到设置并重建面板的新增实浏览器反例 **1失败**：外层按钮误用 delete 模式。
  由原操作同步 mode 后，保留原断言/timeout，最终完整24项通过；不重复 preview/save，只查原 ID。
  日志 `/tmp/dns-rename-remount-red.log`、`/tmp/dns-rename-browser-remount-final.log`。
- 首轮 Rust 完整候选 **1528通过/0失败/7忽略** 后，复审补 mixed legacy 反例真实 **0通过/1失败**：
  clientEdits=None、旧全局标记与 profile DNS 共存时，实际优先 profile 值却错误允许 prepared。
  该反例没有执行 save。按实际 wrapper 来源修正 global 豁免，补多配置/带引用覆盖零写入拒绝及
  纯 legacy global 正例；原反例不放宽，最终完整检查重新执行，不借首轮结果代替修复后结果。
  日志 `/tmp/dns-rename-legacy-red.log`。
- 首次提交的pre-commit类型检查0错误/0警告，但commit-msg因标题中的大写DNS违反subject-case拒绝；
  改为全小写标题后正常重提，不跳过Husky或commitlint。原拒绝日志 `/tmp/dns-rename-commit.log`。

所有变更/删除回归仅使用隔离 TempDir/AppConfig/SQLite，未操作用户配置或真实隧道。
没有 core、依赖、锁、版本或 CI YAML 变更；不使用用户 Mac、不创建签名凭据、不合并或发布。
Android API24/Galaxy S8目标、四项许可缺口、iPhone/真实手机未验收状态保持。
原生VPN/core/平台生命周期装配仍明确未支持，跨配置/托管来源及投影整合并未完成，不能称完整A13或移动端完成。

修复后的最终 locked workspace/all-features Rust 完整测试：**1529通过/0失败/7忽略，45组**，
GUI lib **1255/0/5**；14项新重命名用例及原18项DNS迁移均通过，最终fmt/check/Clippy全部通过。
日志 `/tmp/dns-rename-rust-final-tests.log`；最终fmt、全目标全特性check及Clippy的收据
分别为 `/tmp/dns-rename-rust-final-{fmt,check,clippy}.log`，既有警告不隐藏。
七项实际ignored为：`supplied_zero_confirms_config_application_without_restarting`、
`real_kernel_version_matrix`、`real_kernel_bind_failure_is_never_reported_ready`、
`real_kernel_ready_and_lifetime_pipe_shutdown_release_listener`、
`live_clash_subscription_rule_providers_compile_to_verified_zrs`、
`supplied_zero_accepts_the_production_observation_contract`及Lifecycle doctest。
前六项依赖显式真实内核/捕获或订阅环境；没有将忽略当作通过，未执行本地WebKit或手机验收。
