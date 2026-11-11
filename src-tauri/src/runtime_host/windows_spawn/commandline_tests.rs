use super::*;

fn line(arguments: &[&str]) -> String {
    let encoded = encode(arguments.iter().map(|arg| arg.encode_utf16().collect())).unwrap();
    assert_eq!(encoded.last(), Some(&0));
    String::from_utf16(&encoded[..encoded.len() - 1]).unwrap()
}

#[test]
fn windows_arguments_keep_spaces_empty_values_quotes_and_trailing_backslashes() {
    assert_eq!(line(&["zero.exe", "run", ""]), r#""zero.exe" "run" """#);
    assert_eq!(
        line(&[r"C:\中文目录\zero.exe"]),
        r#""C:\中文目录\zero.exe""#
    );
    assert_eq!(line(&[r"C:\path with space\"]), r#""C:\path with space\\""#);
    assert_eq!(line(&["a\"b"]), "\"a\\\"b\"");
    assert_eq!(line(&["a\\\"b"]), "\"a\\\\\\\"b\"");
}

#[test]
fn windows_arguments_reject_nul_and_preserve_non_unicode_utf16_units() {
    assert!(encode([vec![b'a' as u16, 0, b'b' as u16]]).is_err());
    assert_eq!(
        encode([vec![0xd800]]).unwrap(),
        [b'"' as u16, 0xd800, b'"' as u16, 0]
    );
}
