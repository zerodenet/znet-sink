use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

pub const CONFIGURATION_MAX_BYTES: u32 = 16 * 1024 * 1024;
pub const SETTINGS_MAX_BYTES: u32 = 2 * 1024 * 1024;
pub const DIAGNOSTIC_MAX_BYTES: u32 = 10 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("native_io_invalid_request: {0}")]
    InvalidRequest(&'static str),
    #[error("native_io_invalid_receipt: {0}")]
    InvalidReceipt(&'static str),
    #[error("native_io_unavailable")]
    Unavailable,
    // Never include OS paths, URLs or native exception text in public errors.
    #[error("native_io_bridge_result_unknown")]
    BridgeResultUnknown,
    #[error("native_io_busy")]
    Busy,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy)]
pub enum ImportPurpose {
    Configuration,
    Settings,
}
impl ImportPurpose {
    fn wire(self) -> &'static str {
        match self {
            Self::Configuration => "configuration-import",
            Self::Settings => "settings-import",
        }
    }
    fn budget(self) -> u32 {
        match self {
            Self::Configuration => CONFIGURATION_MAX_BYTES,
            Self::Settings => SETTINGS_MAX_BYTES,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum ExportPurpose {
    OrdinaryConfiguration,
    RedactedDiagnostic,
    Settings,
}
impl ExportPurpose {
    fn wire(self) -> &'static str {
        match self {
            Self::OrdinaryConfiguration => "ordinary-configuration-export",
            Self::RedactedDiagnostic => "redacted-diagnostic-export",
            Self::Settings => "settings-export",
        }
    }
    fn budget(self) -> u32 {
        match self {
            Self::OrdinaryConfiguration => CONFIGURATION_MAX_BYTES,
            Self::RedactedDiagnostic => DIAGNOSTIC_MAX_BYTES,
            Self::Settings => SETTINGS_MAX_BYTES,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum ExportOwnership {
    OrdinaryProfile,
    HostGenerated,
    ProtectedPlugin,
}

/// Host-produced export only. Callers must use their secret-excluding settings serializer
/// or diagnostic redactor BEFORE preparation. This boundary cannot infer semantic redaction.
#[derive(Clone)]
pub struct PreparedExport {
    purpose: ExportPurpose,
    file_name: String,
    mime_type: String,
    bytes: Vec<u8>,
    credentials_acknowledged: bool,
}
impl PreparedExport {
    pub fn new(
        purpose: ExportPurpose,
        ownership: ExportOwnership,
        file_name: &str,
        mime_type: &str,
        bytes: Vec<u8>,
        credentials_acknowledged: bool,
    ) -> Result<Self> {
        match (purpose, ownership) {
            (_, ExportOwnership::ProtectedPlugin) => {
                return Err(Error::InvalidRequest("protected_material_export_forbidden"))
            }
            (ExportPurpose::OrdinaryConfiguration, ExportOwnership::OrdinaryProfile)
            | (
                ExportPurpose::RedactedDiagnostic | ExportPurpose::Settings,
                ExportOwnership::HostGenerated,
            ) => {}
            _ => return Err(Error::InvalidRequest("export_ownership_mismatch")),
        }
        if matches!(purpose, ExportPurpose::OrdinaryConfiguration) && !credentials_acknowledged {
            return Err(Error::InvalidRequest("credential_warning_not_acknowledged"));
        }
        if file_name.is_empty()
            || file_name.len() > 128
            || file_name.starts_with('.')
            || !file_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(Error::InvalidRequest("invalid_file_name"));
        }
        if !matches!(
            mime_type,
            "application/json" | "text/plain" | "application/zip"
        ) || (!matches!(purpose, ExportPurpose::RedactedDiagnostic)
            && mime_type != "application/json")
        {
            return Err(Error::InvalidRequest("invalid_mime_type"));
        }
        if bytes.len() > purpose.budget() as usize {
            return Err(Error::InvalidRequest("size_budget_exceeded"));
        }
        Ok(Self {
            purpose,
            file_name: file_name.into(),
            mime_type: mime_type.into(),
            bytes,
            credentials_acknowledged,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReceiptStatus {
    Read,
    Saved,
    ShareAccepted,
    Cancelled,
    Failed,
    ResultUnknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub operation_id: String,
    pub status: ReceiptStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_read: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
impl Receipt {
    pub fn read_bytes(&self) -> Result<Option<Vec<u8>>> {
        if self.status != ReceiptStatus::Read {
            return Ok(None);
        }
        let bytes = STANDARD
            .decode(
                self.content_base64
                    .as_deref()
                    .ok_or(Error::InvalidReceipt("missing_content"))?,
            )
            .map_err(|_| Error::InvalidReceipt("invalid_base64"))?;
        if self.bytes_read != u32::try_from(bytes.len()).ok() {
            return Err(Error::InvalidReceipt("size_mismatch"));
        }
        Ok(Some(bytes))
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Request<'a> {
    operation_id: &'a str,
    purpose: &'static str,
    maximum_bytes: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    credentials_acknowledged: Option<bool>,
}
#[derive(Clone, Copy)]
pub(crate) enum Action {
    Read,
    Save,
    Share,
}
impl Action {
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Read => "pickRead",
            Self::Save => "save",
            Self::Share => "share",
        }
    }
}
fn operation_id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
impl<'a> Request<'a> {
    pub(crate) fn read(id: &'a str, purpose: ImportPurpose, maximum_bytes: u32) -> Result<Self> {
        if !operation_id_valid(id) {
            return Err(Error::InvalidRequest("invalid_operation_id"));
        }
        if maximum_bytes == 0 || maximum_bytes > purpose.budget() {
            return Err(Error::InvalidRequest("invalid_size_budget"));
        }
        Ok(Self {
            operation_id: id,
            purpose: purpose.wire(),
            maximum_bytes,
            file_name: None,
            mime_type: None,
            payload_base64: None,
            credentials_acknowledged: None,
        })
    }
    pub(crate) fn export(id: &'a str, export: &'a PreparedExport) -> Result<Self> {
        if !operation_id_valid(id) {
            return Err(Error::InvalidRequest("invalid_operation_id"));
        }
        Ok(Self {
            operation_id: id,
            purpose: export.purpose.wire(),
            maximum_bytes: export.purpose.budget(),
            file_name: Some(&export.file_name),
            mime_type: Some(&export.mime_type),
            payload_base64: Some(STANDARD.encode(&export.bytes)),
            credentials_acknowledged: Some(export.credentials_acknowledged),
        })
    }
    pub(crate) fn validate(&self, receipt: Receipt, action: Action) -> Result<Receipt> {
        if receipt.operation_id != self.operation_id {
            return Err(Error::InvalidReceipt("operation_identity_mismatch"));
        }
        if receipt.reason.as_ref().is_some_and(|r| {
            r.len() > 96
                || !r
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        }) {
            return Err(Error::InvalidReceipt("unsafe_reason"));
        }
        let allowed = matches!(
            receipt.status,
            ReceiptStatus::Cancelled | ReceiptStatus::Failed | ReceiptStatus::ResultUnknown
        ) || matches!(
            (action, receipt.status),
            (Action::Read, ReceiptStatus::Read)
                | (Action::Save, ReceiptStatus::Saved)
                | (Action::Share, ReceiptStatus::ShareAccepted)
        );
        if !allowed {
            return Err(Error::InvalidReceipt("unexpected_status"));
        }
        if receipt.status == ReceiptStatus::Read {
            // Bound encoded content before allocation as well as decoded bytes afterwards.
            let encoded = receipt
                .content_base64
                .as_deref()
                .ok_or(Error::InvalidReceipt("missing_content"))?;
            if encoded.len() > (self.maximum_bytes as usize).div_ceil(3) * 4 {
                return Err(Error::InvalidReceipt("size_budget_exceeded"));
            }
            let bytes = receipt
                .read_bytes()?
                .ok_or(Error::InvalidReceipt("missing_content"))?;
            if bytes.len() > self.maximum_bytes as usize {
                return Err(Error::InvalidReceipt("size_budget_exceeded"));
            }
        } else if receipt.content_base64.is_some() || receipt.bytes_read.is_some() {
            return Err(Error::InvalidReceipt("unexpected_content"));
        }
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_and_unacknowledged_exports_are_rejected() {
        assert!(PreparedExport::new(
            ExportPurpose::OrdinaryConfiguration,
            ExportOwnership::ProtectedPlugin,
            "a.json",
            "application/json",
            vec![],
            true
        )
        .is_err());
        assert!(PreparedExport::new(
            ExportPurpose::OrdinaryConfiguration,
            ExportOwnership::OrdinaryProfile,
            "a.json",
            "application/json",
            vec![],
            false
        )
        .is_err());
        assert!(PreparedExport::new(
            ExportPurpose::Settings,
            ExportOwnership::OrdinaryProfile,
            "a.json",
            "application/json",
            vec![],
            true
        )
        .is_err());
    }
    #[test]
    fn filenames_mime_and_budgets_are_checked_before_native_io() {
        for name in ["../secret", ".hidden", "a/b", "a:b", ""] {
            assert!(PreparedExport::new(
                ExportPurpose::Settings,
                ExportOwnership::HostGenerated,
                name,
                "application/json",
                vec![],
                false
            )
            .is_err());
        }
        assert!(PreparedExport::new(
            ExportPurpose::Settings,
            ExportOwnership::HostGenerated,
            "a.json",
            "text/plain",
            vec![],
            false
        )
        .is_err());
        assert!(Request::read("op", ImportPurpose::Settings, SETTINGS_MAX_BYTES + 1).is_err());
        assert!(Request::read("file:/op", ImportPurpose::Settings, 1).is_err());
    }
    fn receipt(status: ReceiptStatus) -> Receipt {
        Receipt {
            operation_id: "op".into(),
            status,
            bytes_read: None,
            content_base64: None,
            reason: None,
        }
    }
    #[test]
    fn stale_and_cross_action_receipts_cannot_claim_success() {
        let req = Request::read("op", ImportPurpose::Configuration, 8).unwrap();
        let mut stale = receipt(ReceiptStatus::Read);
        stale.operation_id = "old".into();
        assert!(req.validate(stale, Action::Read).is_err());
        assert!(req
            .validate(receipt(ReceiptStatus::Saved), Action::Read)
            .is_err());
        assert!(req
            .validate(receipt(ReceiptStatus::Cancelled), Action::Read)
            .is_ok());
        assert!(req
            .validate(receipt(ReceiptStatus::ResultUnknown), Action::Read)
            .is_ok());
    }
    #[test]
    fn partial_oversized_or_cancelled_content_is_rejected() {
        let req = Request::read("op", ImportPurpose::Settings, 3).unwrap();
        let mut r = receipt(ReceiptStatus::Read);
        r.content_base64 = Some(STANDARD.encode(b"four"));
        r.bytes_read = Some(4);
        assert!(req.validate(r.clone(), Action::Read).is_err());
        r.content_base64 = Some(STANDARD.encode(b"ok"));
        r.bytes_read = Some(1);
        assert!(req.validate(r.clone(), Action::Read).is_err());
        r.status = ReceiptStatus::Cancelled;
        assert!(req.validate(r, Action::Read).is_err());
    }
    #[test]
    fn verified_read_is_lossless_and_native_errors_cannot_leak_uris() {
        let req = Request::read("op", ImportPurpose::Settings, 4).unwrap();
        let mut r = receipt(ReceiptStatus::Read);
        r.content_base64 = Some(STANDARD.encode([0, 255, 10]));
        r.bytes_read = Some(3);
        assert_eq!(
            req.validate(r, Action::Read).unwrap().read_bytes().unwrap(),
            Some(vec![0, 255, 10])
        );
        let mut r = receipt(ReceiptStatus::Failed);
        r.reason = Some("content://secret".into());
        assert!(req.validate(r, Action::Read).is_err());
    }
}
