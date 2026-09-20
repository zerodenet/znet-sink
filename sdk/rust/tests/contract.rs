use std::sync::Mutex;
use znet_sink_plugin_sdk::{
    Budget, Call, Capability, Client, Method, Reply, Request, Transport, SDK_VERSION,
};

#[test]
fn scopes_and_budgets_fail_closed() {
    let valid = Call {
        version: SDK_VERSION,
        request: Request {
            capability: Capability::BrowserOpen,
            scope: "https://example.com".into(),
        },
        method: Method::BrowserOpen,
        budget: Budget::default(),
        arguments: serde_json::json!({"url":"https://example.com/account"}),
    };
    assert!(valid.validate());
    assert!(!Call {
        request: Request {
            capability: Capability::BrowserOpen,
            scope: "https://example.com/path".into()
        },
        ..valid.clone()
    }
    .validate());
    assert!(Request {
        capability: Capability::NetworkConfiguredRequest,
        scope: "provider_origin".into(),
    }
    .capability
    .accepts_scope("provider_origin"));
    assert!(!Capability::NetworkConfiguredRequest.accepts_scope("https://example.com"));
    assert!(Capability::SubscriptionsManage.accepts_scope("self"));
    assert!(!Call {
        budget: Budget {
            timeout_ms: 0,
            ..Budget::default()
        },
        ..valid.clone()
    }
    .validate());
    assert!(!Call {
        version: SDK_VERSION + 1,
        ..valid
    }
    .validate());
}

#[derive(Default)]
struct Capture(Mutex<Vec<Call>>);
impl Transport for Capture {
    type Error = ();
    fn send(&self, call: Call) -> Result<Reply, Self::Error> {
        self.0.lock().unwrap().push(call);
        Ok(Reply::success(serde_json::Value::Bool(true)))
    }
}

#[test]
fn typed_client_constructs_capability_requests() {
    let client = Client::new(Capture::default());
    let reply = client.storage_put("state", "session", "b3BhcXVl").unwrap();
    assert!(reply.ok);
    let calls = client.transport().0.lock().unwrap();
    assert_eq!(calls[0].request.capability, Capability::StorageWrite);
    assert_eq!(calls[0].request.scope, "self");
    assert_eq!(calls[0].method, Method::StoragePut);
}
