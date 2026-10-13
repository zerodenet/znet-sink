//! Host provenance for the small public telemetry subset of diagnostic frames.
//! Response bodies cannot name their own method to acquire this classification.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DebugTelemetry {
    QueryStats,
    QueryTrafficStat,
    QueryTrafficStats,
    EventStats,
    EventScopes,
    EventHostInterfaces,
}

impl DebugTelemetry {
    /// Called on the original request by the host, only after its response ID
    /// has been paired and verified. Commands and mixed/unknown queries cannot
    /// establish telemetry provenance.
    pub(crate) fn from_query(request: &Value) -> Option<Self> {
        if request["type"] != "query" {
            return None;
        }
        let body = request.get("request")?.as_object()?;
        if body.len() != 1 {
            return None;
        }
        match zero_api::QueryRequest::deserialize(request.get("request")?).ok()? {
            zero_api::QueryRequest::Stats(_) if body["stats"].as_object()?.is_empty() => {
                Some(Self::QueryStats)
            }
            zero_api::QueryRequest::TrafficStat(_) => Some(Self::QueryTrafficStat),
            zero_api::QueryRequest::TrafficStats(_) => Some(Self::QueryTrafficStats),
            _ => None,
        }
    }

    /// Only canonical typed Zero events received on the kernel transport are
    /// eligible. Arbitrary diagnostic/configuration events stay unclassified.
    pub(crate) fn from_event(frame: &Value) -> Option<Self> {
        let event = zero_api::ApiEvent::<serde::de::IgnoredAny>::deserialize(frame).ok()?;
        if event.schema_id != zero_api::EVENT_SCHEMA_ID {
            return None;
        }
        match event.event_type.as_str() {
            zero_api::event_type::STATS_SAMPLED => Some(Self::EventStats),
            zero_api::event_type::STATS_SCOPES_SAMPLED => Some(Self::EventScopes),
            zero_api::event_type::STATS_HOST_INTERFACES_SAMPLED => Some(Self::EventHostInterfaces),
            _ => None,
        }
    }

    /// The bounded stored copy must still contain the expected successful
    /// telemetry envelope. Truncation, rejection or shape mismatch is omitted,
    /// never interpreted as an observed zero or a different query's result.
    pub(crate) fn body<'a>(&self, frame: &'a super::DebugFrame) -> Option<&'a Value> {
        if frame.direction != "rx" || frame.error.is_some() {
            return None;
        }
        let query_key = match self {
            Self::QueryStats => Some("stats"),
            Self::QueryTrafficStat => Some("traffic_stat"),
            Self::QueryTrafficStats => Some("traffic_stats"),
            _ => None,
        };
        if let Some(key) = query_key {
            if frame.frame_type != "query" || frame.payload["ok"] != true {
                return None;
            }
            let result = frame.payload.get("result")?.as_object()?;
            return (result.len() == 1)
                .then(|| result.get(key))
                .flatten()
                .filter(|body| body.is_object());
        }
        let event_type = match self {
            Self::EventStats => zero_api::event_type::STATS_SAMPLED,
            Self::EventScopes => zero_api::event_type::STATS_SCOPES_SAMPLED,
            Self::EventHostInterfaces => zero_api::event_type::STATS_HOST_INTERFACES_SAMPLED,
            _ => return None,
        };
        if frame.frame_type != "event"
            || frame.payload["schema_id"] != zero_api::EVENT_SCHEMA_ID
            || frame.payload["event_type"] != event_type
        {
            return None;
        }
        frame.payload.get("payload").filter(|body| body.is_object())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn classification_uses_original_exact_query_and_canonical_event_only() {
        assert_eq!(
            DebugTelemetry::from_query(&json!({"type":"query","request":{"traffic_stats":{}}})),
            Some(DebugTelemetry::QueryTrafficStats)
        );
        for request in [
            json!({"type":"command","request":{"stats":{}}}),
            json!({"type":"query","request":{"config":{}},"method":"stats"}),
            json!({"type":"query","request":{"stats":{},"config":{}}}),
            json!({"type":"query","request":{"stats":{"rx_bytes":123}}}),
        ] {
            assert!(DebugTelemetry::from_query(&request).is_none());
        }
        let mut event = serde_json::to_value(zero_api::ApiEvent::new(
            "id",
            zero_api::event_type::STATS_SCOPES_SAMPLED,
            1,
            json!({"scopes":[]}),
        ))
        .unwrap();
        assert_eq!(
            DebugTelemetry::from_event(&event),
            Some(DebugTelemetry::EventScopes)
        );
        event["event_type"] = json!(zero_api::event_type::CONFIG_CHANGED);
        assert!(DebugTelemetry::from_event(&event).is_none());
        event["event_type"] = json!(zero_api::event_type::STATS_SCOPES_SAMPLED);
        event["schema_id"] = json!("guest-canary");
        assert!(DebugTelemetry::from_event(&event).is_none());
    }
}
