//! Strict bounded callback query decoding. Do not silently choose between
//! duplicate authorization fields or replace invalid UTF-8 with new text.
use std::collections::BTreeMap;

fn digit(byte: u8) -> Result<u8, ()> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(()),
    }
}
fn decode(value: &str) -> Result<String, ()> {
    let input = value.as_bytes();
    let mut decoded = Vec::with_capacity(input.len());
    let mut position = 0;
    while position < input.len() {
        match input[position] {
            b'%' => {
                let high = *input.get(position + 1).ok_or(())?;
                let low = *input.get(position + 2).ok_or(())?;
                decoded.push(digit(high)? * 16 + digit(low)?);
                position += 3;
            }
            b'+' => {
                decoded.push(b' ');
                position += 1;
            }
            byte => {
                decoded.push(byte);
                position += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| ())
}

pub(super) fn parse(target: &str, id: &str) -> Result<BTreeMap<String, String>, ()> {
    if target.len() > 8192 || target.contains('#') {
        return Err(());
    }
    let suffix = target.strip_prefix(&format!("/callback/{id}")).ok_or(())?;
    let query = if suffix.is_empty() {
        ""
    } else {
        suffix.strip_prefix('?').ok_or(())?
    };
    let mut result = BTreeMap::new();
    if query.is_empty() {
        return Ok(result);
    }
    for pair in query.split('&') {
        if result.len() >= 32 {
            return Err(());
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode(key)?;
        let value = decode(value)?;
        if key.is_empty()
            || key.len() > 128
            || value.len() > 4096
            || result.insert(key, value).is_some()
        {
            return Err(());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opaque_values_are_exact_and_duplicate_or_corrupt_fields_are_rejected() {
        let query = parse(
            "/callback/fixed?code=a%2Bb%26c%3Dd&state=two+words&utf8=%E4%B8%AD",
            "fixed",
        )
        .unwrap();
        assert_eq!(query["code"], "a+b&c=d");
        assert_eq!(query["state"], "two words");
        assert_eq!(query["utf8"], "中");
        for input in [
            "/callback/fixed?state=first&state=second",
            "/callback/fixed?state=first&%73tate=second",
            "/callback/fixed?code=%FF",
            "/callback/fixed?code=%C0%AF",
            "/callback/fixed?code=%",
            "/callback/fixed?code=%2X",
            "/callback/fixed/../fixed?code=opaque",
            "/callback/fixed-other",
            "/callback/fixed?code=opaque#other",
            "/callback/fixed?=unnamed",
        ] {
            assert!(
                parse(input, "fixed").is_err(),
                "malformed callback accepted"
            );
        }
    }
    #[test]
    fn decoded_fields_and_pair_count_are_bounded_before_publication() {
        let accepted = (0..32)
            .map(|n| format!("k{n}=v"))
            .collect::<Vec<_>>()
            .join("&");
        assert_eq!(
            parse(&format!("/callback/fixed?{accepted}"), "fixed")
                .unwrap()
                .len(),
            32
        );
        assert!(parse(&format!("/callback/fixed?{accepted}&extra=v"), "fixed").is_err());
        assert!(parse(&format!("/callback/fixed?{}=v", "x".repeat(129)), "fixed").is_err());
        assert!(parse(
            &format!("/callback/fixed?code={}", "x".repeat(4097)),
            "fixed"
        )
        .is_err());
        assert!(parse("/callback/fixed-other?code=opaque", "fixed").is_err());
    }
}
