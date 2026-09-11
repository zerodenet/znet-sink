use super::*;

fn completed_frame(id: u64, at_ms: u64) -> DebugFrame {
    DebugFrame {
        id,
        at_ms,
        direction: "rx".to_string(),
        frame_type: "event".to_string(),
        payload: serde_json::json!({
            "eventType": "connection.closed",
            "payload": {
                "record": {
                    "flowId": format!("flow-{id}"),
                    "network": "tcp",
                    "path": { "outbound": { "tag": "proxy-a" } },
                    "result": { "outcome": "success" }
                }
            }
        }),
        elapsed_ms: None,
        error: None,
    }
}

#[test]
fn detects_only_completed_connection_events() {
    let completed = completed_frame(1, now_unix_ms());
    assert!(is_completed_connection_frame(&completed));

    let mut updated = completed.clone();
    updated.payload["eventType"] = serde_json::json!("connection.updated");
    assert!(!is_completed_connection_frame(&updated));
}

#[test]
fn filters_connection_history_before_paging() {
    let frame = completed_frame(1, now_unix_ms());
    let query = DebugFrameQuery {
        protocol: Some("tcp".to_string()),
        outbound: Some("proxy-a".to_string()),
        outcome: Some("success".to_string()),
        search: Some("flow-1".to_string()),
        ..DebugFrameQuery::default()
    };
    assert!(matches_query(&frame, &query));
}

#[test]
fn capture_time_and_close_reason_filter_before_paging() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.jsonl");
    for id in 1..=6 {
        let mut frame = completed_frame(id, id * 1000);
        frame.payload["payload"]["record"]["result"]["closeReason"] = serde_json::json!("eof");
        append_to_path(&path, &frame).unwrap();
    }
    let query = DebugFrameQuery {
        captured_after_ms: Some(2000),
        captured_before_ms: Some(5000),
        outcome: Some("eof".into()),
        limit: Some(2),
        ..Default::default()
    };
    let page = query_page_from_path(&path, &query).unwrap();
    assert_eq!(
        page.items.iter().map(|f| f.id).collect::<Vec<_>>(),
        vec![4, 5]
    );
    assert!(page.has_more);
    let summary = page.history.unwrap();
    assert_eq!(summary.retained_records, 6);
    assert_eq!(summary.matched_records, 4);
    assert_eq!(summary.oldest_captured_at_ms, Some(1000));
    assert_eq!(summary.newest_captured_at_ms, Some(6000));
    let next = query_page_from_path(
        &path,
        &DebugFrameQuery {
            before_id: Some(4),
            ..query
        },
    )
    .unwrap();
    assert_eq!(
        next.items.iter().map(|f| f.id).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert!(!next.has_more);
}

#[test]
fn rotation_retains_bounded_history_and_latest_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.jsonl");
    let now = now_unix_ms();
    append_to_path(&path, &completed_frame(0, now - HISTORY_MAX_AGE_MS - 1)).unwrap();
    for id in 1..=HISTORY_RECORD_LIMIT as u64 + 2 {
        append_to_path(&path, &completed_frame(id, now)).unwrap();
    }
    rotate_path(&path).unwrap();
    let page = query_page_from_path(
        &path,
        &DebugFrameQuery {
            limit: Some(usize::MAX),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.items.len(), HISTORY_RECORD_LIMIT);
    assert_eq!(page.items[0].id, 3);
    assert_eq!(
        report::latest_id_from_path(&path).unwrap(),
        Some(HISTORY_RECORD_LIMIT as u64 + 2)
    );
    assert!(!page.has_more);
}

#[test]
fn byte_limit_drops_oversized_record_and_keeps_later_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.jsonl");
    let mut huge = completed_frame(1, now_unix_ms());
    huge.payload["payload"]["record"]["extra"] =
        serde_json::json!("x".repeat(HISTORY_MAX_BYTES as usize));
    append_to_path(&path, &huge).unwrap();
    append_to_path(&path, &completed_frame(2, now_unix_ms())).unwrap();
    rotate_path(&path).unwrap();
    let page = query_page_from_path(&path, &DebugFrameQuery::default()).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, 2);
    assert!(fs::metadata(&path).unwrap().len() <= HISTORY_MAX_BYTES);
}

#[test]
fn export_contains_filtered_snapshot_and_explicit_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.jsonl");
    for id in 1..=3 {
        append_to_path(&path, &completed_frame(id, id * 1000)).unwrap();
    }
    let query = DebugFrameQuery {
        captured_after_ms: Some(2000),
        ..Default::default()
    };
    let page = query_page_from_path(&path, &query).unwrap();
    let result = report::export_page(page, query, &dir.path().join("exports")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fs::read(result.path).unwrap()).unwrap();
    assert_eq!(result.records, 2);
    assert_eq!(value["records"].as_array().unwrap().len(), 2);
    assert_eq!(value["summary"]["completeness"], "unknown");
    assert_eq!(value["filters"]["capturedAfterMs"], 2000);
    assert_eq!(value["hasMore"], false);
}
