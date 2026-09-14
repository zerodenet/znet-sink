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
