use super::{directory::Registration, Result, MAX_PACKAGE_BYTES};
use crate::contract::{sha256, Component, Manifest, Target};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
const DOMAIN: &[u8] = b"znet-sink.plugin-package.v1\0";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub format: String,
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
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceComponent {
    pub manifest: Manifest,
    pub source: String,
}
pub struct VerifiedPackage {
    pub id: String,
    pub version: String,
    pub digest: String,
    pub components: Vec<Component>,
}

/// Author-side packaging. The private seed is never part of a package or directory.
pub fn sign(payload: &[u8], seed: &[u8; 32]) -> Result<Vec<u8>> {
    if payload.len() > 2 * 1024 * 1024 {
        return Err("payload exceeds limit".into());
    }
    let _: Payload = serde_json::from_slice(payload)?;
    let key = SigningKey::from_bytes(seed);
    let mut message = DOMAIN.to_vec();
    message.extend_from_slice(payload);
    Ok(serde_json::to_vec(&Envelope {
        format: "znet-sink.plugin-package.v1".into(),
        payload: STANDARD.encode(payload),
        signature: STANDARD.encode(key.sign(&message).to_bytes()),
    })?)
}
pub fn verify(bytes: &[u8], registration: &Registration) -> Result<VerifiedPackage> {
    registration.validate()?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err("package exceeds limit".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    if envelope.format != "znet-sink.plugin-package.v1" {
        return Err("unsupported package format".into());
    }
    let payload = STANDARD.decode(envelope.payload)?;
    if payload.len() > 2 * 1024 * 1024 {
        return Err("payload exceeds limit".into());
    }
    let public_key: [u8; 32] = STANDARD
        .decode(&registration.publisher.public_key)?
        .try_into()
        .map_err(|_| "invalid key")?;
    let signature = Signature::from_slice(&STANDARD.decode(envelope.signature)?)?;
    let mut message = DOMAIN.to_vec();
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
            if !registration
                .capabilities
                .iter()
                .any(|c| Some(c.as_str()) == name.as_str())
            {
                return Err("package exceeds registered capability ceiling".into());
            }
        }
        components.push(Component::load(&serde_json::to_vec(m)?, &source.source)?);
    }
    Ok(VerifiedPackage {
        id: payload.plugin_id,
        version: payload.version,
        digest: sha256(bytes),
        components,
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
