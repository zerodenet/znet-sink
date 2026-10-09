use super::*;
use serde_json::json;

fn frame(id: u64, frame_type: &str, bytes: usize) -> DebugFrame {
    DebugFrame {
        id,
        at_ms: id,
        direction: "rx".into(),
        frame_type: frame_type.into(),
        payload: json!({"data": "x".repeat(bytes)}),
        elapsed_ms: None,
        error: None,
    }
}

#[test]
fn rotates_segments_by_bytes_and_pages_across_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("debug.log.jsonl");
    for id in 1..=600 {
        append_to_path(
            &path,
            &frame(id, if id % 2 == 0 { "event" } else { "query" }, 16 * 1024),
        )
        .unwrap();
    }
    let paths = segment_paths(&path);
    let bytes: u64 = paths
        .iter()
        .filter_map(|path| fs::metadata(path).ok())
        .map(|file| {
            assert!(file.len() <= SEGMENT_BYTES);
            file.len()
        })
        .sum();
    assert!(bytes <= 5 * SEGMENT_BYTES);
    let query = DebugFrameQuery {
        frame_type: Some("query".into()),
        limit: Some(1000),
        ..Default::default()
    };
    let page = query_page_from_path(&path, &query).unwrap();
    assert!(page.has_more); // Byte cap applies even with a large requested count.
    assert!(page.items.len() < 40);
    assert_eq!(page.items.last().unwrap().id, 599);
    assert!(page.oldest_available_id.unwrap() > 1);
    let previous = query_page_from_path(
        &path,
        &DebugFrameQuery {
            before_id: Some(page.items[0].id),
            ..query
        },
    )
    .unwrap();
    assert!(previous.items.last().unwrap().id < page.items[0].id);
    assert_eq!(latest_id_from_path(&path).unwrap(), Some(600));
    clear_path(&path).unwrap();
    assert!(paths.iter().all(|path| !path.exists()));
}

#[test]
fn append_does_not_parse_or_rewrite_existing_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("debug.log.jsonl");
    fs::write(&path, b"malformed-record-kept-verbatim\n").unwrap();
    append_to_path(&path, &frame(100, "event", 1)).unwrap();
    assert!(fs::read(&path)
        .unwrap()
        .starts_with(b"malformed-record-kept-verbatim\n"));
    assert_eq!(
        query_page_from_path(&path, &DebugFrameQuery::default())
            .unwrap()
            .items
            .len(),
        1
    );
}

#[test]
fn legacy_migration_reads_only_a_bounded_tail_and_skips_oversized_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("debug.log.jsonl");
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(&vec![b'x'; 4 * SEGMENT_BYTES as usize])
        .unwrap();
    file.write_all(b"\n").unwrap();
    writeln!(
        file,
        "{}",
        serde_json::to_string(&frame(800, "query", 5)).unwrap()
    )
    .unwrap();
    bound_legacy_file(&path).unwrap();
    assert!(fs::metadata(&path).unwrap().len() <= SEGMENT_BYTES);
    assert_eq!(latest_id_from_path(&path).unwrap(), Some(800));
    assert_eq!(
        query_page_from_path(&path, &DebugFrameQuery::default())
            .unwrap()
            .items[0]
            .id,
        800
    );
}

#[test]
fn rejects_an_oversized_record_and_seeds_from_highest_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("debug.log.jsonl");
    for id in [12, 40, 3] {
        append_to_path(&path, &frame(id, "query", 1)).unwrap();
    }
    assert_eq!(latest_id_from_path(&path).unwrap(), Some(40));
    let size = fs::metadata(&path).unwrap().len();
    assert!(append_to_path(&path, &frame(41, "event", RECORD_BYTES)).is_err());
    assert_eq!(fs::metadata(&path).unwrap().len(), size);
}
