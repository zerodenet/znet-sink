use super::{verify_payload, PageKind, Payload, SourceComponent, SourcePage, VerifiedPackage};
use crate::{
    contract::{sha256, Manifest},
    distribution::{directory::Registration, Result, MAX_PACKAGE_BYTES},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const DOMAIN_V3: &[u8] = b"znet-sink.plugin-package.v3\0";
const MANIFEST_PATH: &str = "plugin.json";
const SIGNATURE_PATH: &str = "META-INF/signature.json";
const MAX_FILES: usize = 256;
const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: usize = 32 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileDigest {
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationComponent {
    pub id: String,
    pub manifest: String,
    pub entry: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationPage {
    pub id: String,
    pub title: String,
    pub kind: PageKind,
    pub entry: String,
}

/// Signed root manifest for the v3 application package. `files` is generated
/// by the packer and covers every archive member other than this manifest and
/// the detached signature envelope.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationManifest {
    pub schema_version: u32,
    pub host: String,
    pub plugin_id: String,
    pub version: String,
    pub components: Vec<ApplicationComponent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<ApplicationPage>,
    #[serde(default)]
    pub files: BTreeMap<String, FileDigest>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignatureEnvelope {
    format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    registration: Option<Registration>,
    signature: String,
}

struct Archive {
    manifest_bytes: Vec<u8>,
    signature: SignatureEnvelope,
    files: BTreeMap<String, Vec<u8>>,
}

pub(super) fn is_application(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

pub fn sign_application(
    manifest: ApplicationManifest,
    files: BTreeMap<String, Vec<u8>>,
    seed: &[u8; 32],
) -> Result<Vec<u8>> {
    sign(manifest, files, seed, None)
}

pub fn sign_application_with_registration(
    manifest: ApplicationManifest,
    files: BTreeMap<String, Vec<u8>>,
    seed: &[u8; 32],
    registration: Registration,
) -> Result<Vec<u8>> {
    sign(manifest, files, seed, Some(registration))
}

fn sign(
    mut manifest: ApplicationManifest,
    files: BTreeMap<String, Vec<u8>>,
    seed: &[u8; 32],
    registration: Option<Registration>,
) -> Result<Vec<u8>> {
    validate_root(&manifest)?;
    validate_files(&files)?;
    validate_content(&manifest, &files)?;
    let key = SigningKey::from_bytes(seed);
    if let Some(registration) = &registration {
        registration.validate()?;
        if registration.id != manifest.plugin_id
            || STANDARD.decode(&registration.publisher.public_key)?
                != key.verifying_key().to_bytes()
        {
            return Err("package registration does not match manifest or signing key".into());
        }
    }
    manifest.files = files
        .iter()
        .map(|(path, bytes)| {
            (
                path.clone(),
                FileDigest {
                    sha256: sha256(bytes),
                    size: bytes.len() as u64,
                },
            )
        })
        .collect();
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err("application manifest exceeds limit".into());
    }
    let signature = SignatureEnvelope {
        format: "znet-sink.plugin-package.v3".into(),
        signature: STANDARD.encode(
            key.sign(&message(&registration, &manifest_bytes)?)
                .to_bytes(),
        ),
        registration,
    };
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .unix_permissions(0o644);
    writer.start_file(MANIFEST_PATH, options)?;
    writer.write_all(&manifest_bytes)?;
    for (path, bytes) in files {
        writer.start_file(path, options)?;
        writer.write_all(&bytes)?;
    }
    writer.start_file(SIGNATURE_PATH, options)?;
    writer.write_all(&serde_json::to_vec_pretty(&signature)?)?;
    let bytes = writer.finish()?.into_inner();
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err("package exceeds limit".into());
    }
    if let Some(registration) = signature.registration.as_ref() {
        verify(&bytes, registration, false)?;
    }
    Ok(bytes)
}

pub(super) fn embedded_registration(bytes: &[u8]) -> Result<Option<Registration>> {
    Ok(read_archive(bytes)?.signature.registration)
}

pub(super) fn package_id(bytes: &[u8]) -> Result<String> {
    let archive = read_archive(bytes)?;
    let manifest: ApplicationManifest = serde_json::from_slice(&archive.manifest_bytes)?;
    validate_root(&manifest)?;
    Ok(manifest.plugin_id)
}

pub(super) fn verify(
    bytes: &[u8],
    registration: &Registration,
    enforce_marketplace_ceiling: bool,
) -> Result<VerifiedPackage> {
    let archive = read_archive(bytes)?;
    let manifest: ApplicationManifest = serde_json::from_slice(&archive.manifest_bytes)?;
    validate_root(&manifest)?;
    if manifest.plugin_id != registration.id {
        return Err("package identity mismatch".into());
    }
    verify_signature(&archive, registration)?;
    verify_files(&manifest.files, &archive.files)?;

    let payload = build_payload(&manifest, &archive.files)?;
    verify_payload(
        payload,
        registration,
        enforce_marketplace_ceiling,
        sha256(bytes),
        archive.files,
    )
}

fn build_payload(
    manifest: &ApplicationManifest,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Payload> {
    let mut component_ids = BTreeSet::new();
    let mut components = Vec::with_capacity(manifest.components.len());
    for descriptor in &manifest.components {
        if !identifier(&descriptor.id) || !component_ids.insert(descriptor.id.as_str()) {
            return Err("invalid application component descriptor".into());
        }
        let manifest_bytes = files
            .get(&descriptor.manifest)
            .ok_or("component manifest is missing")?;
        let component_manifest: Manifest = serde_json::from_slice(manifest_bytes)?;
        if component_manifest.component_id != descriptor.id
            || component_manifest.plugin_id != manifest.plugin_id
            || component_manifest.version != manifest.version
        {
            return Err("component descriptor identity mismatch".into());
        }
        let source = files
            .get(&descriptor.entry)
            .ok_or("component entry is missing")?;
        components.push(SourceComponent {
            manifest: component_manifest,
            source: String::from_utf8(source.clone())?,
        });
    }
    let pages = manifest
        .pages
        .iter()
        .map(|page| {
            let html = files.get(&page.entry).ok_or("page entry is missing")?;
            Ok(SourcePage {
                id: page.id.clone(),
                title: page.title.clone(),
                kind: page.kind,
                html: String::from_utf8(html.clone())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Payload {
        schema_version: 1,
        host: manifest.host.clone(),
        plugin_id: manifest.plugin_id.clone(),
        version: manifest.version.clone(),
        components,
        pages,
    })
}

fn validate_content(
    manifest: &ApplicationManifest,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let payload = build_payload(manifest, files)?;
    for component in payload.components {
        crate::contract::Component::load(
            &serde_json::to_vec(&component.manifest)?,
            &component.source,
        )?;
    }
    let mut page_ids = BTreeSet::new();
    for page in payload.pages {
        if page.id.is_empty()
            || page.id.len() > 80
            || !page
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || !page_ids.insert(page.id)
            || page.title.is_empty()
            || page.title.len() > 80
            || page.html.is_empty()
            || page.html.len() > 512 * 1024
        {
            return Err("invalid application page".into());
        }
    }
    Ok(())
}

fn verify_signature(archive: &Archive, registration: &Registration) -> Result<()> {
    if archive.signature.format != "znet-sink.plugin-package.v3" {
        return Err("unsupported package format".into());
    }
    if let Some(embedded) = archive.signature.registration.as_ref() {
        embedded.validate()?;
        if embedded.id != registration.id
            || embedded.publisher.public_key != registration.publisher.public_key
        {
            return Err("package embedded publisher identity mismatch".into());
        }
    }
    let public_key: [u8; 32] = STANDARD
        .decode(&registration.publisher.public_key)?
        .try_into()
        .map_err(|_| "invalid key")?;
    let signature = Signature::from_slice(&STANDARD.decode(&archive.signature.signature)?)?;
    VerifyingKey::from_bytes(&public_key)?.verify_strict(
        &message(&archive.signature.registration, &archive.manifest_bytes)?,
        &signature,
    )?;
    Ok(())
}

fn message(registration: &Option<Registration>, manifest: &[u8]) -> Result<Vec<u8>> {
    let mut message = DOMAIN_V3.to_vec();
    if let Some(registration) = registration {
        message.extend_from_slice(&serde_json::to_vec(registration)?);
        message.push(0);
    }
    message.extend_from_slice(manifest);
    Ok(message)
}

fn read_archive(bytes: &[u8]) -> Result<Archive> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err("package exceeds limit".into());
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    if zip.len() < 2 || zip.len() > MAX_FILES + 2 {
        return Err("application package file count invalid".into());
    }
    let mut entries = BTreeMap::new();
    let mut total = 0_usize;
    for index in 0..zip.len() {
        let mut file = zip.by_index(index)?;
        if file.encrypted()
            || file.is_dir()
            || file.is_symlink()
            || !matches!(
                file.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err("unsupported application package entry".into());
        }
        let raw_name = std::str::from_utf8(file.name_raw())?.to_owned();
        validate_path(&raw_name)?;
        let file_size = file.size() as usize;
        if file.enclosed_name().is_none() || file_size > MAX_FILE_BYTES {
            return Err("application package entry exceeds limit".into());
        }
        total = total
            .checked_add(file_size)
            .ok_or("application package expands beyond limit")?;
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err("application package expands beyond limit".into());
        }
        let mut data = Vec::with_capacity(file_size);
        file.by_ref()
            .take((MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut data)?;
        if data.len() != file_size || entries.insert(raw_name, data).is_some() {
            return Err("duplicate or invalid application package entry".into());
        }
    }
    let manifest_bytes = entries
        .remove(MANIFEST_PATH)
        .ok_or("application manifest is missing")?;
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err("application manifest exceeds limit".into());
    }
    let signature = serde_json::from_slice(
        &entries
            .remove(SIGNATURE_PATH)
            .ok_or("application signature is missing")?,
    )?;
    Ok(Archive {
        manifest_bytes,
        signature,
        files: entries,
    })
}

fn validate_root(manifest: &ApplicationManifest) -> Result<()> {
    if manifest.schema_version != 3
        || manifest.host != "znet-sink"
        || !identifier(&manifest.plugin_id)
        || semver::Version::parse(&manifest.version).is_err()
        || manifest.components.is_empty()
        || manifest.components.len() > 8
        || manifest.pages.len() > 4
    {
        return Err("invalid application manifest".into());
    }
    for component in &manifest.components {
        validate_path(&component.manifest)?;
        validate_path(&component.entry)?;
    }
    for page in &manifest.pages {
        validate_path(&page.entry)?;
    }
    Ok(())
}

fn validate_files(files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err("application package file count invalid".into());
    }
    let mut total = 0_usize;
    for (path, bytes) in files {
        validate_path(path)?;
        if path == MANIFEST_PATH || path == SIGNATURE_PATH || bytes.len() > MAX_FILE_BYTES {
            return Err("invalid application package file".into());
        }
        total = total
            .checked_add(bytes.len())
            .ok_or("application package expands beyond limit")?;
    }
    if total > MAX_UNCOMPRESSED_BYTES {
        return Err("application package expands beyond limit".into());
    }
    Ok(())
}

fn verify_files(
    declared: &BTreeMap<String, FileDigest>,
    actual: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    if declared.len() != actual.len() || declared.is_empty() || declared.len() > MAX_FILES {
        return Err("application package file index mismatch".into());
    }
    for (path, digest) in declared {
        validate_path(path)?;
        let bytes = actual
            .get(path)
            .ok_or("application package file index mismatch")?;
        if digest.size != bytes.len() as u64 || digest.sha256 != sha256(bytes) {
            return Err("application package file digest mismatch".into());
        }
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 240
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.bytes().any(|byte| byte.is_ascii_control())
        || path.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.len() > 80
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        return Err("invalid application package path".into());
    }
    Ok(())
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}
