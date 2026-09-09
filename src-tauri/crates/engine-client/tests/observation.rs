use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Mutex;
use znet_engine_client::{FlowFilter, ObservationClient, ObservationError, QueryTransport};

struct FakeTransport(Mutex<VecDeque<(Value, Result<Value, &'static str>)>>);

impl QueryTransport for FakeTransport {
    type Error = &'static str;
    async fn query(&self, request: Value) -> Result<Value, Self::Error> {
        let (expected, response) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected query/retry");
        assert_eq!(request, expected);
        response
    }
}
impl Drop for FakeTransport {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.0.lock().unwrap().is_empty(), "missing queries");
        }
    }
}
fn client(steps: Vec<(Value, Result<Value, &'static str>)>) -> ObservationClient<FakeTransport> {
    ObservationClient::new(FakeTransport(Mutex::new(steps.into())))
}
fn runtime(id: &str, revision: u64) -> Value {
    json!({"core_instance_id":id,"config_revision":revision})
}
fn baseline(after: Value, optional_fail: bool) -> ObservationClient<FakeTransport> {
    client(vec![
        (json!({"runtime":{}}), Ok(json!({"runtime":runtime("a",1)}))),
        (
            json!({"stats":{}}),
            if optional_fail {
                Err("unsupported")
            } else {
                Ok(json!({"stats":{"up":10}}))
            },
        ),
        (
            json!({"policies":{}}),
            if optional_fail {
                Err("unsupported")
            } else {
                Ok(json!({"policies":[]}))
            },
        ),
        (
            json!({"active_flows":{"limit":500,"filter":{}}}),
            Ok(json!({"active_flows":{"items":[]}})),
        ),
        (json!({"runtime":{}}), Ok(after)),
    ])
}

#[tokio::test]
async fn lists_preserve_wire_contract_and_normalize_bounds_and_filters() {
    let client = client(vec![
        (
            json!({"active_flows":{"limit":500,"filter":{"inbound_tag":"tun"}}}),
            Ok(json!({"active_flows":{"items":[1]}})),
        ),
        (
            json!({"recent_flows":{"limit":1,"filter":{"principal_key":"user"}}}),
            Ok(json!({"items":[2]})),
        ),
    ]);
    assert_eq!(
        client
            .active(&FlowFilter {
                limit: Some(900),
                inbound_tag: Some(" tun ".into()),
                principal_key: Some(" ".into())
            })
            .await
            .unwrap(),
        json!({"items":[1]})
    );
    assert_eq!(
        client
            .recent(&FlowFilter {
                limit: Some(0),
                principal_key: Some(" user ".into()),
                ..Default::default()
            })
            .await
            .unwrap(),
        json!({"items":[2]})
    );
}
#[tokio::test]
async fn detail_validates_input_before_transport_and_preserves_kernel_error() {
    let client = client(vec![(json!({"flow":{"flow_id":"a"}}), Err("not_found"))]);
    assert_eq!(
        client.detail(" ").await.unwrap_err(),
        ObservationError::InvalidInput("flowId must not be empty")
    );
    assert_eq!(
        client.detail(" a ").await.unwrap_err(),
        ObservationError::Transport("not_found")
    );
}
#[tokio::test]
async fn baseline_accepts_coherent_runtime_and_configuration() {
    let result = baseline(runtime("a", 1), false).snapshot().await.unwrap();
    assert_eq!(result.runtime, runtime("a", 1));
    assert_eq!(result.connections, json!({"items":[]}));
    assert_eq!(result.stats, Some(json!({"up":10})));
}
#[tokio::test]
async fn baseline_rejects_runtime_or_config_change() {
    for after in [runtime("b", 1), runtime("a", 2)] {
        assert!(matches!(
            baseline(after, false).snapshot().await,
            Err(ObservationError::RuntimeChanged)
        ));
    }
}
#[tokio::test]
async fn missing_identity_is_not_a_successful_empty_baseline() {
    let client = client(vec![(json!({"runtime":{}}), Ok(json!({})))]);
    assert!(matches!(
        client.snapshot().await,
        Err(ObservationError::InvalidResponse(_))
    ));
}
#[tokio::test]
async fn optional_query_failures_remain_missing() {
    let result = baseline(runtime("a", 1), true).snapshot().await.unwrap();
    assert!(result.stats.is_none());
    assert!(result.policies.is_none());
}
#[tokio::test]
async fn disconnected_transport_is_not_retried_in_a_bound_read() {
    let client = client(vec![(json!({"runtime":{}}), Err("closed"))]);
    assert!(matches!(
        client.snapshot().await,
        Err(ObservationError::Transport("closed"))
    ));
}
#[tokio::test]
async fn camel_case_runtime_identity_is_supported() {
    baseline(json!({"coreInstanceId":"a","configRevision":"1"}), false)
        .snapshot()
        .await
        .unwrap();
}
