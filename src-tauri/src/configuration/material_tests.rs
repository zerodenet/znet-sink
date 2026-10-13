use super::*;
#[test]
fn file_and_pasted_configuration_enter_the_same_preparation_path() {
    let manager = Manager::default();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.json");
    std::fs::write(&path, b"{\"inbounds\":[],\"outbounds\":[]}").unwrap();
    let file = import(&manager, None, Some(path.to_string_lossy().into_owned())).unwrap();
    let pasted = import(&manager, Some(file.to_string()), None).unwrap();
    assert_eq!(file, pasted);
    let operations = manager.operations();
    assert_eq!(
        operations
            .iter()
            .filter(|o| o.capability == "configuration.prepare")
            .count(),
        2
    );
    assert_eq!(
        operations
            .iter()
            .filter(|o| o.capability == "file.read")
            .count(),
        1
    );
}

#[test]
fn mobile_path_import_is_rejected_before_any_file_lease_and_offers_paste() {
    let manager = Manager::default();
    let error = import_for_platform(
        &manager,
        None,
        Some("content://documents/source".into()),
        true,
    )
    .unwrap_err();
    assert_eq!(error.code, "mobile_file_import_not_adapted");
    assert_eq!(error.details.unwrap()["alternative"], "paste-content");
    assert!(manager.operations().is_empty());
}

#[test]
fn mobile_paste_import_never_consults_an_unauthorized_path() {
    let manager = Manager::default();
    let value = import_for_platform(
        &manager,
        Some("{\"inbounds\":[],\"outbounds\":[],\"custom_metadata\":\"preserved\"}".into()),
        Some("/unselected/desktop/config.json".into()),
        true,
    )
    .unwrap();
    assert_eq!(value["custom_metadata"], "preserved");
    assert!(manager
        .operations()
        .iter()
        .all(|operation| operation.capability != "file.read"));
}
