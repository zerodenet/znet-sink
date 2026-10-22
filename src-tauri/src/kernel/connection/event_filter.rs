//! Per-listener delivery filtering. Never changes the shared runtime subscription.
use crate::errors::{AppError, AppResult};
use serde_json::Value;

#[derive(Clone, Default)]
pub(crate) struct EventFilter {
    kinds: Vec<String>,
}
impl EventFilter {
    pub(crate) fn new(kinds: Option<Vec<String>>) -> AppResult<Self> {
        let kinds = kinds.unwrap_or_default();
        if kinds.len() > 32
            || kinds.iter().any(|kind| {
                kind.is_empty() || kind.len() > 128 || kind.chars().any(char::is_whitespace)
            })
        {
            return Err(AppError::invalid_argument(
                "event filter exceeds the listener budget",
            ));
        }
        Ok(Self { kinds })
    }
    pub(crate) fn matches(&self, event: &Value, gui_kind: Option<&str>) -> bool {
        if self.kinds.is_empty() {
            return true;
        }
        let kind = event
            .get("event_type")
            .or_else(|| event.get("eventType"))
            .or_else(|| event.get("type"))
            .and_then(Value::as_str);
        self.kinds
            .iter()
            .any(|wanted| Some(wanted.as_str()) == kind || Some(wanted.as_str()) == gui_kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listener_filters_are_bounded_and_do_not_match_payload_claims() {
        let event =
            serde_json::json!({"event_type":"flow.started","payload":{"type":"stats.sampled"}});
        assert!(EventFilter::new(None).unwrap().matches(&event, None));
        assert!(EventFilter::new(Some(vec!["flow.started".into()]))
            .unwrap()
            .matches(&event, None));
        assert!(EventFilter::new(Some(vec!["core.flowStarted".into()]))
            .unwrap()
            .matches(&event, Some("core.flowStarted")));
        assert!(!EventFilter::new(Some(vec!["stats.sampled".into()]))
            .unwrap()
            .matches(&event, None));
        assert!(EventFilter::new(Some(vec!["x".repeat(129)])).is_err());
        assert!(EventFilter::new(Some(vec!["x".into(); 33])).is_err());
    }
}
