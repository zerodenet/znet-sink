use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::SigningKey;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use znet_plugin_sandbox::{
    contract::{sha256, Component, Target, MAX_MANIFEST_BYTES, MAX_SOURCE_BYTES},
    distribution::{
        directory::Registration,
        package::{self, ApplicationManifest, Payload, SourceComponent},
        remote::{ReleaseMetadata, Remote},
        store::Store,
        Result,
    },
};
fn main() {
    if let Err(e) = run() {
        eprintln!("plugin: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "catalog" => println!("{}", serde_json::to_string_pretty(&Remote::new()?.directory()?)?),
        [cmd, id] if cmd == "releases" => {
            let remote = Remote::new()?; let directory = remote.directory()?;
            println!("{}", serde_json::to_string_pretty(&remote.releases(directory.find(id)?)?)?);
        }
        [cmd, root] if cmd == "list" => {
            for id in Store::new(root)?.list()? { println!("{id}\tinstalled (not enabled)"); }
        }
        [cmd, root, id] if cmd == "remove" => { Store::new(root)?.uninstall(id)?; println!("removed {id}"); }
        [cmd, root, id, tag] if cmd == "install" => {
            let remote = Remote::new()?; let directory = remote.directory()?; let registration = directory.find(id)?;
            let release = remote.release(registration, tag)?;
            let bytes = remote.download(registration, &release)?;
            let trusted = remote.registration_directory()?;
            let package = Store::new(root)?.install(&bytes, trusted.find(id)?, &Target::native_desktop()?, env!("CARGO_PKG_VERSION"))?;
            println!("installed {} {} {} (not enabled)", package.id, package.version, package.digest);
        }
        [cmd, root, id] if cmd == "rollback" || cmd == "inspect" => {
            let directory = Remote::new()?.registration_directory()?; let registration = directory.find(id)?; let store = Store::new(root)?;
            let package = if cmd == "rollback" { store.rollback(registration, &Target::native_desktop()?, env!("CARGO_PKG_VERSION"))? } else { store.current(registration)? };
            println!("{} {} {} (not enabled)", package.id, package.version, package.digest);
            for component in package.components { println!("{}", serde_json::to_string_pretty(component.manifest())?); }
        }
        [cmd, manifest, source, seed, output, metadata] if cmd == "pack" => pack(manifest, source, seed, output, metadata, None)?,
        [cmd, manifest, source, seed, output, metadata, registration] if cmd == "pack" => pack(manifest, source, seed, output, metadata, Some(registration))?,
        [cmd, root, seed, output, metadata] if cmd == "pack-app" => pack_app(root, seed, output, metadata, None)?,
        [cmd, root, seed, output, metadata, registration] if cmd == "pack-app" => pack_app(root, seed, output, metadata, Some(registration))?,
        _ => return Err("usage: znet-plugin catalog | releases ID | list ROOT | install ROOT ID TAG | inspect ROOT ID | rollback ROOT ID | remove ROOT ID | pack MANIFEST SOURCE SEED_FILE PACKAGE_OUT METADATA_OUT [REGISTRATION_JSON] | pack-app APP_ROOT SEED_FILE PACKAGE_OUT METADATA_OUT [REGISTRATION_JSON]".into()),
    }
    Ok(())
}
fn read(path: &str, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("input exceeds limit".into());
    }
    Ok(bytes)
}
fn pack(
    manifest: &str,
    source: &str,
    seed: &str,
    output: &str,
    metadata: &str,
    registration: Option<&str>,
) -> Result<()> {
    if Path::new(output).exists() || Path::new(metadata).exists() || output == metadata {
        return Err("output must be a new file".into());
    }
    let source = String::from_utf8(read(source, MAX_SOURCE_BYTES)?)?;
    let component = Component::load(&read(manifest, MAX_MANIFEST_BYTES)?, &source)?;
    let manifest = component.manifest().clone();
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: manifest.plugin_id.clone(),
        version: manifest.version.clone(),
        components: vec![SourceComponent { manifest, source }],
        pages: Vec::new(),
    };
    let seed: [u8; 32] = read(seed, 32)?
        .try_into()
        .map_err(|_| "seed must contain exactly 32 raw bytes")?;
    let payload_bytes = serde_json::to_vec(&payload)?;
    let package = if let Some(path) = registration {
        let registration: Registration = serde_json::from_slice(&read(path, 64 * 1024)?)?;
        package::sign_with_registration(&payload_bytes, &seed, registration)?
    } else {
        package::sign(&payload_bytes, &seed)?
    };
    write_outputs(
        output,
        metadata,
        package,
        payload.host,
        payload.plugin_id,
        payload.version,
        seed,
    )
}

fn pack_app(
    root: &str,
    seed: &str,
    output: &str,
    metadata: &str,
    registration: Option<&str>,
) -> Result<()> {
    if Path::new(output).exists() || Path::new(metadata).exists() || output == metadata {
        return Err("output must be a new file".into());
    }
    let root = std::fs::canonicalize(root)?;
    let root_metadata = std::fs::symlink_metadata(&root)?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("application root must be a real directory".into());
    }
    reject_app_control_path(&root, Path::new(seed), true)?;
    reject_app_control_path(&root, Path::new(output), false)?;
    reject_app_control_path(&root, Path::new(metadata), false)?;
    if let Some(registration) = registration {
        reject_app_control_path(&root, Path::new(registration), true)?;
    }
    let manifest_path = root.join("plugin.json");
    let mut manifest: ApplicationManifest =
        serde_json::from_slice(&read_path(&manifest_path, 256 * 1024)?)?;
    if !manifest.files.is_empty() {
        return Err("plugin.json files index is generated by pack-app and must be empty".into());
    }
    let mut files = BTreeMap::new();
    collect_files(&root, &root, &mut files)?;
    files.remove("plugin.json");
    if files.remove("META-INF/signature.json").is_some() {
        return Err("application source must not contain META-INF/signature.json".into());
    }
    let seed: [u8; 32] = read(seed, 32)?
        .try_into()
        .map_err(|_| "seed must contain exactly 32 raw bytes")?;
    let package = if let Some(path) = registration {
        let registration: Registration = serde_json::from_slice(&read(path, 64 * 1024)?)?;
        package::sign_application_with_registration(manifest.clone(), files, &seed, registration)?
    } else {
        package::sign_application(manifest.clone(), files, &seed)?
    };
    write_outputs(
        output,
        metadata,
        package,
        std::mem::take(&mut manifest.host),
        std::mem::take(&mut manifest.plugin_id),
        std::mem::take(&mut manifest.version),
        seed,
    )
}

fn write_outputs(
    output: &str,
    metadata: &str,
    package: Vec<u8>,
    host: String,
    plugin_id: String,
    version: String,
    seed: [u8; 32],
) -> Result<()> {
    let metadata_value = ReleaseMetadata {
        schema_version: 1,
        host,
        plugin_id,
        version,
        asset: Path::new(output)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid package filename")?
            .into(),
        sha256: sha256(&package),
    };
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?
        .write_all(&package)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(metadata)?
        .write_all(&serde_json::to_vec_pretty(&metadata_value)?)?;
    println!(
        "publisher public key: {}",
        STANDARD.encode(SigningKey::from_bytes(&seed).verifying_key().to_bytes())
    );
    Ok(())
}

fn read_path(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let path = path
        .to_str()
        .ok_or("application source path is not valid UTF-8")?;
    read(path, limit)
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err("application source symlinks are not allowed".into());
        }
        if metadata.is_dir() {
            collect_files(root, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            return Err("application source contains an unsupported entry".into());
        }
        let relative = path.strip_prefix(root)?;
        let package_path = portable_path(relative)?;
        if files
            .insert(package_path, read_path(&path, 4 * 1024 * 1024)?)
            .is_some()
        {
            return Err("duplicate application source path".into());
        }
        if files.len() > 257 {
            return Err("application source contains too many files".into());
        }
    }
    Ok(())
}

fn portable_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            return Err("application source path is not portable".into());
        };
        parts.push(
            part.to_str()
                .ok_or("application source path is not valid UTF-8")?,
        );
    }
    if parts.is_empty() {
        return Err("application source path is empty".into());
    }
    Ok(parts.join("/"))
}

fn reject_app_control_path(root: &Path, path: &Path, must_exist: bool) -> Result<()> {
    let resolved = if must_exist {
        std::fs::canonicalize(path)?
    } else {
        let parent = path
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = std::fs::canonicalize(parent)?;
        parent.join(path.file_name().ok_or("invalid application output path")?)
    };
    if resolved.starts_with(root) {
        return Err(
            "seed, registration and output files must be outside the application root".into(),
        );
    }
    Ok(())
}
