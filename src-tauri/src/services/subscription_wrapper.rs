#[path = "subscription.rs"]
mod original;

pub use original::{
    get, list, parse_subscription_content, remove, spawn_auto_sync_scheduler, sync, sync_all,
    upsert, ParsedSubscriptionConfig, SyncAllOutcome,
};

#[cfg(test)]
mod wrapper_tests {
    use crate::models::subscription::SubscriptionUpsert;

    #[test]
    fn automatic_source_format_is_not_rewritten() {
        let input = SubscriptionUpsert {
            id: None,
            name: "Example".to_string(),
            url: "https://example.com/subscription".to_string(),
            format: Some("auto".to_string()),
            user_agent: None,
            enabled: Some(true),
            auto_update: Some(true),
            update_interval_secs: Some(3600),
        };

        assert_eq!(input.format.as_deref(), Some("auto"));
        assert!(input.user_agent.is_none());
    }
}
