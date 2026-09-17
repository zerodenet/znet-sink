use super::{directory::Registration, Result, MAX_PACKAGE_BYTES};
use crate::contract::{sha256, Component, Manifest, Target};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
const DOMAIN_V1: &[u8] = b"znet-sink.plugin-package.v1\0";
const DOMAIN_V2: &[u8] = b"znet-sink.plugin-package.v2\0";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub format: String,
    /// A self-contained publisher declaration for explicit local installation.
    /// Marketplace installation still uses the registry as its trust root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration: Option<Registration>,
    pub payload: String,
    pub signature: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub schema_version: u32,
    pub host: String,
    pub plugin_id: String,
    pub version: String,
    pub components: Vec<SourceComponent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<SourcePage>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceComponent {
    pub manifest: Manifest,
    pub source: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePage {
    pub id: String,
    pub title: String,
    pub kind: PageKind,
    pub html: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    Management,
}
#[derive(Clone)]
pub struct VerifiedPage {
    pub id: String,
    pub title: String,
    pub kind: PageKind,
    pub html: String,
}
pub struct VerifiedPackage {
    pub id: String,
    pub version: String,
    pub digest: String,
    pub components: Vec<Component>,
    pub pages: Vec<VerifiedPage>,
}

/// Author-side packaging. The private seed is never part of a package or directory.
pub fn sign(payload: &[u8], seed: &[u8; 32]) -> Result<Vec<u8>> {
    if payload.len() > 2 * 1024 * 1024 {
        return Err("payload exceeds limit".into());
    }
    let _: Payload = serde_json::from_slice(payload)?;
    let key = SigningKey::from_bytes(seed);
    let mut message = DOMAIN_V1.to_vec();
    message.extend_from_slice(payload);
    Ok(serde_json::to_vec(&Envelope {
        format: "znet-sink.plugin-package.v1".into(),
        registration: None,
        payload: STANDARD.encode(payload),
        signature: STANDARD.encode(key.sign(&message).to_bytes()),
    })?)
}

/// Author-side packaging for a package that can be installed without a
/// marketplace listing. The signed registration is shown to the user as a
/// first-use trust decision; it does not create a marketplace registration.
pub fn sign_with_registration(
    payload: &[u8],
    seed: &[u8; 32],
    registration: Registration,
) -> Result<Vec<u8>> {
    if payload.len() > 2 * 1024 * 1024 {
        return Err("payload exceeds limit".into());
    }
    let parsed: Payload = serde_json::from_slice(payload)?;
    registration.validate()?;
    let key = SigningKey::from_bytes(seed);
    if parsed.plugin_id != registration.id
        || STANDARD.decode(&registration.publisher.public_key)? != key.verifying_key().to_bytes()
    {
        return Err("package registration does not match payload or signing key".into());
    }
    let registration_bytes = serde_json::to_vec(&registration)?;
    let mut message = DOMAIN_V2.to_vec();
    message.extend_from_slice(&registration_bytes);
    message.push(0);
    message.extend_from_slice(payload);
    Ok(serde_json::to_vec(&Envelope {
        format: "znet-sink.plugin-package.v2".into(),
        registration: Some(registration),
        payload: STANDARD.encode(payload),
        signature: STANDARD.encode(key.sign(&message).to_bytes()),
    })?)
}

pub fn embedded_registration(bytes: &[u8]) -> Result<Option<Registration>> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err("package exceeds limit".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    match envelope.format.as_str() {
        "znet-sink.plugin-package.v1" if envelope.registration.is_none() => Ok(None),
        "znet-sink.plugin-package.v2" => envelope
            .registration
            .map(Some)
            .ok_or_else(|| "package has no embedded publisher registration".into()),
        _ => Err("unsupported package format".into()),
    }
}

pub fn verify(bytes: &[u8], registration: &Registration) -> Result<VerifiedPackage> {
    verify_with_policy(bytes, registration, true)
}

/// Verify a package selected directly by the user. Publisher identity and the
/// signature are still mandatory, while marketplace capability/surface
/// ceilings do not apply to this explicit local trust path.
pub fn verify_local(bytes: &[u8], registration: &Registration) -> Result<VerifiedPackage> {
    verify_with_policy(bytes, registration, false)
}

fn verify_with_policy(
    bytes: &[u8],
    registration: &Registration,
    enforce_marketplace_ceiling: bool,
) -> Result<VerifiedPackage> {
    registration.validate()?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err("package exceeds limit".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    let payload = STANDARD.decode(envelope.payload)?;
    if payload.len() > 2 * 1024 * 1024 {
        return Err("payload exceeds limit".into());
    }
    let public_key: [u8; 32] = STANDARD
        .decode(&registration.publisher.public_key)?
        .try_into()
        .map_err(|_| "invalid key")?;
    let signature = Signature::from_slice(&STANDARD.decode(envelope.signature)?)?;
    let mut message = match envelope.format.as_str() {
        "znet-sink.plugin-package.v1" if envelope.registration.is_none() => DOMAIN_V1.to_vec(),
        "znet-sink.plugin-package.v2" => {
            let embedded = envelope
                .registration
                .as_ref()
                .ok_or("package has no embedded publisher registration")?;
            embedded.validate()?;
            if embedded.id != registration.id
                || embedded.publisher.public_key != registration.publisher.public_key
            {
                return Err("package embedded publisher identity mismatch".into());
            }
            let mut message = DOMAIN_V2.to_vec();
            message.extend_from_slice(&serde_json::to_vec(embedded)?);
            message.push(0);
            message
        }
        _ => return Err("unsupported package format".into()),
    };
    message.extend_from_slice(&payload);
    VerifyingKey::from_bytes(&public_key)?.verify_strict(&message, &signature)?;
    let payload: Payload = serde_json::from_slice(&payload)?;
    if payload.schema_version != 1
        || payload.host != "znet-sink"
        || payload.plugin_id != registration.id
        || payload.components.is_empty()
        || payload.components.len() > 8
    {
        return Err("package identity or component count invalid".into());
    }
    let mut ids = BTreeSet::new();
    let mut components = Vec::new();
    for source in payload.components {
        let m = &source.manifest;
        if m.plugin_id != payload.plugin_id
            || m.version != payload.version
            || !ids.insert(m.component_id.clone())
        {
            return Err("component identity mismatch".into());
        }
        for permission in m.required.iter().chain(&m.optional) {
            let name = serde_json::to_value(permission.capability)?;
            if enforce_marketplace_ceiling
                && !registration
                    .capabilities
                    .iter()
                    .any(|c| Some(c.as_str()) == name.as_str())
            {
                return Err("package exceeds registered capability ceiling".into());
            }
        }
        components.push(Component::load(&serde_json::to_vec(m)?, &source.source)?);
    }
    if payload.pages.len() > 4
        || (enforce_marketplace_ceiling
            && !payload.pages.is_empty()
            && !registration
                .surfaces
                .iter()
                .any(|surface| surface == "znet-sink.ui.management.v1"))
    {
        return Err("package page exceeds registered surface ceiling".into());
    }
    let mut page_ids = BTreeSet::new();
    let mut pages = Vec::new();
    for page in payload.pages {
        if page.id.is_empty()
            || page.id.len() > 80
            || !page
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || !page_ids.insert(page.id.clone())
            || page.title.is_empty()
            || page.title.len() > 80
            || page.html.is_empty()
            || page.html.len() > 512 * 1024
        {
            return Err("invalid plugin page".into());
        }
        pages.push(VerifiedPage {
            id: page.id,
            title: page.title,
            kind: page.kind,
            html: page.html,
        });
    }
    Ok(VerifiedPackage {
        id: payload.plugin_id,
        version: payload.version,
        digest: sha256(bytes),
        components,
        pages,
    })
}
impl VerifiedPackage {
    /// Package installation requires at least one usable device component. Each execution rechecks isolation.
    pub fn compatible(&self, target: &Target, version: &str) -> Result<()> {
        if self
            .components
            .iter()
            .any(|c| c.compatible_device(target, version).is_ok())
        {
            Ok(())
        } else {
            Err("no component supports this device and host version".into())
        }
    }
}
