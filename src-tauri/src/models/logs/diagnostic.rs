//! Host-selected public event categories. No free text, identity, URL or payload
//! can enter this type. Stored categories are private-JSONL history, not signed
//! evidence or authority for a live control/health decision.
use super::LogSource;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "event",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(crate) enum HostDiagnosticEvent {
    Process(ProcessEvent),
    Subscription(ScheduleEvent),
    RuleSet(ScheduleEvent),
    RuleDependency(RuleDependencyEvent),
    NodeSnapshot(NodeSnapshotEvent),
    NodeProbe(ProbeEvent),
    PreviousConnectionCleanupIncomplete,
    Plugin {
        action: PluginAction,
        outcome: HostOutcome,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProcessEvent {
    Spawning,
    SpawnFailed,
    ReadinessFailed,
    Started,
    Stopped,
    Exited,
    Crashed,
    ConfigurationRejected,
    ProxyPortUnavailable,
    PollFailed,
    StopFailed,
    SharedConnectionRecovered,
    Unresponsive,
    AutoRestartScheduled,
    AutoRestartExhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ScheduleEvent {
    StorageLoadFailed,
    StorageSaveFailed,
    CollectDueFailed,
    Succeeded,
    Updated,
    Unchanged,
    SkippedConflict,
    RetryScheduled,
    RetryLimitReached,
    RuntimeRecomposeFailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RuleDependencyEvent {
    PreviousArtifactRetained,
    ArtifactUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NodeSnapshotEvent {
    Runtime,
    StaticFallback,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProbeEvent {
    Requested,
    Reachable,
    Unreachable,
    RequestFailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum HostOutcome {
    Succeeded,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PluginAction {
    Run,
    Uninstall,
    ManagedSubscriptionApply,
    ManagedSubscriptionMetadata,
    ManagedSubscriptionSync,
    ManagedSubscriptionRemove,
    ProtectedMaterialLoad,
    ScheduledComponentAction,
    ComponentAction,
}
impl HostDiagnosticEvent {
    pub(crate) fn source(self) -> LogSource {
        match self {
            Self::Plugin { .. } => LogSource::Plugin,
            Self::Process(ProcessEvent::SpawnFailed | ProcessEvent::ReadinessFailed) => {
                LogSource::Core
            }
            _ => LogSource::App,
        }
    }
    pub(crate) fn valid_for_source(self, source: &LogSource) -> bool {
        &self.source() == source
    }
    /// A completely reconstructed copy. Never return entry.message or fields.
    pub(crate) fn public_message(self) -> &'static str {
        match self {
            Self::Process(event) => match event {
                ProcessEvent::Spawning => "正在创建内核进程",
                ProcessEvent::SpawnFailed => "内核进程创建失败",
                ProcessEvent::ReadinessFailed => "内核进程启动就绪检查失败",
                ProcessEvent::Started => "内核进程已启动",
                ProcessEvent::Stopped => "内核进程已停止",
                ProcessEvent::Exited => "内核进程已退出",
                ProcessEvent::Crashed => "内核进程异常退出",
                ProcessEvent::ConfigurationRejected => "内核启动配置预检被拒绝",
                ProcessEvent::ProxyPortUnavailable => "本地代理监听端口不可用",
                ProcessEvent::PollFailed => "内核进程状态读取失败",
                ProcessEvent::StopFailed => "内核进程停止失败",
                ProcessEvent::SharedConnectionRecovered => "内核共享控制连接已恢复",
                ProcessEvent::Unresponsive => "内核共享控制连接连续检查失败",
                ProcessEvent::AutoRestartScheduled => "已安排内核进程自动重启尝试",
                ProcessEvent::AutoRestartExhausted => "内核进程自动重启次数已耗尽",
            },
            Self::Subscription(event) => match event {
                ScheduleEvent::StorageLoadFailed => "订阅调度记录读取失败",
                ScheduleEvent::StorageSaveFailed => "订阅调度记录保存失败",
                ScheduleEvent::CollectDueFailed => "订阅到期检查失败",
                ScheduleEvent::Succeeded => "订阅自动同步已完成",
                ScheduleEvent::SkippedConflict => "订阅自动同步因并发操作跳过",
                ScheduleEvent::RetryScheduled => "订阅自动同步失败，已安排重试",
                ScheduleEvent::RetryLimitReached => "订阅自动同步失败，已进入重试冷却",
                _ => "订阅调度状态已记录",
            },
            Self::RuleSet(event) => match event {
                ScheduleEvent::StorageLoadFailed => "规则集调度记录读取失败",
                ScheduleEvent::StorageSaveFailed => "规则集调度记录保存失败",
                ScheduleEvent::CollectDueFailed => "规则集到期检查失败",
                ScheduleEvent::Updated => "规则集自动更新已保存新产物",
                ScheduleEvent::Unchanged => "规则集自动检查内容未变化",
                ScheduleEvent::SkippedConflict => "规则集自动更新因并发操作跳过",
                ScheduleEvent::RetryScheduled => "规则集自动更新失败，已安排重试",
                ScheduleEvent::RetryLimitReached => "规则集自动更新失败，已进入重试冷却",
                ScheduleEvent::RuntimeRecomposeFailed => "规则集已保存，但运行配置重组合失败",
                _ => "规则集调度状态已记录",
            },
            Self::RuleDependency(RuleDependencyEvent::PreviousArtifactRetained) => {
                "订阅规则依赖更新失败，保留上次已验证产物"
            }
            Self::RuleDependency(RuleDependencyEvent::ArtifactUnavailable) => {
                "规则依赖更新失败，未取得可用产物"
            }
            Self::NodeSnapshot(NodeSnapshotEvent::Runtime) => "节点页面运行快照已刷新",
            Self::NodeSnapshot(NodeSnapshotEvent::StaticFallback) => "节点页面快照使用本地配置回退",
            Self::NodeProbe(ProbeEvent::Requested) => "已收到节点测速请求",
            Self::NodeProbe(ProbeEvent::Reachable) => "节点测速结果为可达",
            Self::NodeProbe(ProbeEvent::Unreachable) => "内核返回节点测速未成功",
            Self::NodeProbe(ProbeEvent::RequestFailed) => "节点测速请求未成功完成",
            Self::PreviousConnectionCleanupIncomplete => "配置切换后的旧连接清理尚未全部完成",
            Self::Plugin {
                outcome: HostOutcome::Succeeded,
                ..
            } => "插件宿主操作已完成",
            Self::Plugin {
                outcome: HostOutcome::Failed,
                ..
            } => "插件宿主操作失败",
        }
    }
}
/// Forward/old/corrupt classification cannot poison an otherwise readable log
/// record or promote fields/message. Only an exact closed schema is accepted.
pub(super) fn deserialize_optional<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<HostDiagnosticEvent>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn closed_event_rejects_unexpected_fields_strings_and_unknown_variants() {
        for input in [
            json!({"kind":"process","event":"started","secret":"canary"}),
            json!({"kind":"process","event":{"started":"canary"}}),
            json!({"kind":"plugin","event":{"action":"run","outcome":"succeeded","secret":"canary"}}),
            json!({"kind":"secret-canary","event":"started"}),
        ] {
            assert!(serde_json::from_value::<HostDiagnosticEvent>(input).is_err());
        }
    }
    #[test]
    fn typed_plugin_action_contains_only_closed_public_data() {
        let event = HostDiagnosticEvent::Plugin {
            action: PluginAction::ScheduledComponentAction,
            outcome: HostOutcome::Failed,
        };
        assert!(!serde_json::to_string(&event).unwrap().contains("canary"));
        assert!(!event.public_message().contains("canary"));
        assert_eq!(event.source(), LogSource::Plugin);
        assert!(!event.valid_for_source(&LogSource::App));
    }
}
