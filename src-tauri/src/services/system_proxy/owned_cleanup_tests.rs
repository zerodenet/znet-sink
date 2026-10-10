use super::*;

#[test]
fn macos_cleanup_checks_each_service_and_protocol_without_restoring_backup() {
    let owned = vec![("127.0.0.1".into(), 7890)];
    for (write, output, expected) in [
        (
            "-setwebproxystate",
            "Enabled: Yes\nServer: 127.0.0.1\nPort: 7890",
            true,
        ),
        (
            "-setsecurewebproxystate",
            "Enabled: Yes\nServer: 127.0.0.1\nPort: 1080",
            false,
        ),
        (
            "-setsocksfirewallproxystate",
            "Enabled: Yes\nServer: 127.0.0.1\nPort: 7890",
            true,
        ),
        (
            "-setwebproxystate",
            "Enabled: No\nServer: 127.0.0.1\nPort: 7890",
            false,
        ),
    ] {
        let command =
            matching_macos_command("Wi-Fi's && unchanged", write, output, &owned).unwrap();
        assert_eq!(command.is_some(), expected);
        if let Some(command) = command {
            assert_eq!(command, [write, "Wi-Fi's && unchanged", "off"]);
        }
    }
    assert!(
        matching_macos_command("Wi-Fi", "-setwebproxystate", "invalid response", &owned).is_err()
    );
}
