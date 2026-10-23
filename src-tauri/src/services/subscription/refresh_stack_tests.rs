use super::super::*;
use std::{
    cell::{Cell, RefCell},
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

const CHILD_ENV: &str = "ZNET_SUBSCRIPTION_STACK_CHILD";

#[test]
fn subscription_refresh_on_windows_sized_stack() {
    if std::env::var_os(CHILD_ENV).is_none() {
        let dir = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "services::subscription::original::stack_tests::refresh::subscription_refresh_on_windows_sized_stack", "--nocapture"])
            .env(CHILD_ENV, "1")
            .env("ZNET_SINK_DATA_DIR", dir.path())
            .output().unwrap();
        assert!(
            output.status.success(),
            "bounded-stack child failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    std::thread::Builder::new()
        .name("subscription-1mib".into())
        .stack_size(1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            tauri::async_runtime::set(runtime.handle().clone());
            runtime.block_on(run_fixture());
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn run_fixture() {
    let server = Fixture::start();
    let profile: SubscriptionProfile = serde_json::from_value(json!({
        "id":"stack-fixture", "name":"Stack fixture", "url":format!("{}/subscription",server.url),
        "enabled":true, "kernel":"zero", "format":"zero", "targetProxyConfigId":"fixture-config", "updatedAtUnixMs":1
    })).unwrap();
    let state = AppState::with_domain_data(
        Default::default(),
        vec![],
        vec![profile.clone()],
        vec![],
        vec![],
    );
    let published = RefCell::new(None::<ProxyConfigProfile>);
    let publishes = Cell::new(0);
    for cycle in 0..6 {
        server.cycle.store(cycle, Ordering::SeqCst);
        let old_config = published.borrow().clone();
        let old_rules = lock(state.rule_sets(), "rule_set").unwrap().clone();
        let input = lock(state.subscriptions(), "subscription").unwrap()[0].clone();
        let _guard =
            begin_in_flight(state.subscription_syncs(), "subscription", &input.id).unwrap();
        let result = Box::pin(sync_with_publish(&state, input, |input| {
            let state = &state;
            let published = &published;
            let publishes = &publishes;
            async move {
                publishes.set(publishes.get() + 1);
                assert_eq!(
                    input.content.as_ref().unwrap()["route"]["rules"]
                        .as_array()
                        .unwrap()
                        .len(),
                    31
                );
                crate::services::route_integrity::validate(input.content.as_ref().unwrap())
                    .unwrap();
                let effective = crate::services::rule_overlay::compose_effective_config_for(
                    &state,
                    input.content.as_ref().unwrap(),
                    input.id.as_deref(),
                )
                .unwrap();
                assert_eq!(effective["outbounds"].as_array().unwrap().len(), 24);
                if cycle == 4 {
                    return Err(AppError::invalid_argument(
                        "fixture kernel rejected candidate",
                    ));
                }
                let next = ProxyConfigProfile {
                    id: input.id.unwrap(),
                    name: input.name,
                    kernel: "zero".into(),
                    format: "json".into(),
                    path: None,
                    content: input.content,
                    active: true,
                    policy_selections: Default::default(),
                    managed_source: None,
                    updated_at_unix_ms: now_unix_ms(),
                    capabilities: Default::default(),
                };
                // Native publication is a double; real source parsing, conditional
                // downloads, ZRS verification, composition and metadata are exercised.
                domain_store::save_proxy_configs_to_dir(
                    &crate::services::data_dir().unwrap(),
                    std::slice::from_ref(&next),
                )
                .unwrap();
                *lock(state.proxy_configs(), "proxy_config").unwrap() = vec![next.clone()];
                *published.borrow_mut() = Some(next.clone());
                Ok(next)
            }
        }))
        .await;
        if cycle == 4 || cycle == 5 {
            assert!(result.is_err());
            assert_eq!(
                serde_json::to_value(&*published.borrow()).unwrap(),
                serde_json::to_value(old_config).unwrap()
            );
            assert_eq!(
                serde_json::to_value(&*lock(state.rule_sets(), "rule_set").unwrap()).unwrap(),
                serde_json::to_value(old_rules).unwrap()
            );
        } else {
            result.unwrap();
            assert_eq!(lock(state.rule_sets(), "rule_set").unwrap().len(), 31);
        }
    }
    assert_eq!(
        publishes.get(),
        5,
        "unavailable new dependency must block publication"
    );
    assert_eq!(server.requests.load(Ordering::SeqCst), 192);
    assert_eq!(server.not_modified.load(Ordering::SeqCst), 31 * 4 - 2);
}

struct Fixture {
    url: String,
    cycle: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
    not_modified: Arc<AtomicUsize>,
}
impl Fixture {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let cycle = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(AtomicUsize::new(0));
        let not_modified = Arc::new(AtomicUsize::new(0));
        let control = Arc::clone(&cycle);
        let count = Arc::clone(&requests);
        let cached = Arc::clone(&not_modified);
        let base = url.clone();
        std::thread::spawn(move || {
            let zrs = artifact();
            for incoming in listener.incoming().take(192) {
                let mut stream = incoming.unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    request.push_str(&line);
                }
                let cycle = control.load(Ordering::SeqCst);
                count.fetch_add(1, Ordering::SeqCst);
                let (status, body) = if request.starts_with("GET /subscription ") {
                    (
                        "200 OK",
                        general_purpose::STANDARD
                            .encode(serde_json::to_vec(&source(&base, cycle)).unwrap())
                            .into_bytes(),
                    )
                } else if cycle == 5 && request.starts_with("GET /missing ")
                    || cycle == 3 && request.starts_with("GET /rule/0 ")
                {
                    ("503 Unavailable", b"unavailable".to_vec())
                } else if cycle > 0
                    && cycle != 4
                    && request
                        .to_ascii_lowercase()
                        .contains("if-none-match: \"fixture\"")
                {
                    cached.fetch_add(1, Ordering::SeqCst);
                    ("304 Not Modified", vec![])
                } else {
                    ("200 OK", zrs.clone())
                };
                write!(stream,"HTTP/1.1 {status}\r\nContent-Length: {}\r\nETag: \"fixture\"\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        Self {
            url,
            cycle,
            requests,
            not_modified,
        }
    }
}
fn source(base: &str, cycle: usize) -> Value {
    json!({
        "inbounds":[],
        "outbounds":(0..24).map(|i|json!({"tag":if i==0 {"direct".into()}else{format!("node-{i}")},"protocol":{"type":"direct"}})).collect::<Vec<_>>(),
        "route":{"final":"direct","rules":(0..31).map(|i|json!({"condition":{"type":"rule_set","tag":if cycle==5 && i==0 {"new-rules".into()}else{format!("rules-{i}")}},"outbound":"direct"})).collect::<Vec<_>>(),
            "rule_sets":(0..31).map(|i|json!({"tag":if cycle==5 && i==0 {"new-rules".into()}else{format!("rules-{i}")},"type":"url","format":"zrs","url":if cycle==5 && i==0 {format!("{base}/missing")}else{format!("{base}/rule/{i}")}})).collect::<Vec<_>>()},
        "runtime":{"dns":{"servers":{"alidns":{"type":"doh","host":"223.5.5.5","port":443,"path":"/dns-query","server_name":"dns.alidns.com"}},"default_server":"alidns","answer":{"type":"real"},"policy":{"address_family":"prefer_ipv4"},"cache":{"max_entries":1024}},"fixture_revision":cycle}
    })
}
fn artifact() -> Vec<u8> {
    let value = json!({"version":1,"name":"fixture","rules":[{"type":"domain_suffix","value":"example.test"}]});
    let source = zero_rule::protocol::decode_json(&serde_json::to_vec(&value).unwrap()).unwrap();
    let (compiled, _) = zero_rule::RuleSetCompiler.compile(source).unwrap();
    zero_rule::zrs::encode(&compiled).unwrap()
}
