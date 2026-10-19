//! Secret bridge values have no Debug/Serialize/event/UI implementation.
use crate::{Error, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Serialize)]
pub(super) struct Request {
    pub create: bool,
    #[serde(rename = "smokeNonce", skip_serializing_if = "Option::is_none")]
    pub smoke_nonce: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Response {
    status: String,
    key_base64: Option<String>,
    reason: Option<String>,
}
impl Drop for Response {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        if let Some(value) = self.key_base64.as_mut() {
            value.zeroize();
        }
    }
}
impl Response {
    pub(super) fn take(mut self, create: bool) -> Result<Option<Zeroizing<Vec<u8>>>> {
        if self.reason.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "secure-store-unavailable"
                    | "key-missing"
                    | "key-corrupt"
                    | "protected-data-unavailable"
            )
        }) {
            return Err(Error::InvalidReceipt("invalid_secure_store_receipt"));
        }
        match self.status.as_str() {
            "available" if self.reason.is_none() => {
                let encoded = Zeroizing::new(
                    self.key_base64
                        .take()
                        .ok_or(Error::InvalidReceipt("invalid_secure_store_receipt"))?,
                );
                if encoded.len() != 44 {
                    return Err(Error::InvalidReceipt("invalid_secure_store_receipt"));
                }
                let key = Zeroizing::new(
                    STANDARD
                        .decode(encoded.as_bytes())
                        .map_err(|_| Error::InvalidReceipt("invalid_secure_store_receipt"))?,
                );
                if key.len() != 32 {
                    return Err(Error::InvalidReceipt("invalid_secure_store_receipt"));
                }
                Ok(Some(key))
            }
            "missing" if !create && self.key_base64.is_none() => Ok(None),
            "unavailable" if self.key_base64.is_none() => Err(Error::Unavailable),
            _ => Err(Error::InvalidReceipt("invalid_secure_store_receipt")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn response(status: &str, bytes: Option<&[u8]>) -> Response {
        Response {
            status: status.into(),
            key_base64: bytes.map(|value| STANDARD.encode(value)),
            reason: None,
        }
    }
    #[test]
    fn native_vault_never_substitutes_missing_creation_or_malformed_key() {
        assert!(response("missing", None).take(false).unwrap().is_none());
        assert!(response("missing", None).take(true).is_err());
        for size in [0, 16, 31, 33, 4096] {
            assert!(response("available", Some(&vec![7; size]))
                .take(false)
                .is_err());
        }
        assert_eq!(
            response("available", Some(&[7; 32]))
                .take(false)
                .unwrap()
                .unwrap()
                .len(),
            32
        );
        assert!(response("missing", Some(&[7; 32])).take(false).is_err());
        assert!(response("unavailable", Some(&[7; 32])).take(false).is_err());
        assert!(response("unknown", None).take(false).is_err());
    }
    #[test]
    fn unexpected_fields_or_native_error_details_are_not_exposed() {
        assert!(
            serde_json::from_str::<Response>(r#"{"status":"missing","path":"secret"}"#).is_err()
        );
        let value: Response =
            serde_json::from_str(r#"{"status":"unavailable","reason":"secret-url-token"}"#)
                .unwrap();
        assert!(matches!(
            value.take(false),
            Err(Error::InvalidReceipt("invalid_secure_store_receipt"))
        ));
    }
}
