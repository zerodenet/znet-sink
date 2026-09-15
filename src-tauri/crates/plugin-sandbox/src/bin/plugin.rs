use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::SigningKey;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use znet_plugin_sandbox::{
    contract::{sha256, Component, Target, MAX_MANIFEST_BYTES, MAX_SOURCE_BYTES},
    distribution::{
        package::{self, Payload, SourceComponent},
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
        [cmd, manifest, source, seed, output, metadata] if cmd == "pack" => pack(manifest, source, seed, output, metadata)?,
        _ => return Err("usage: znet-plugin catalog | releases ID | list ROOT | install ROOT ID TAG | inspect ROOT ID | rollback ROOT ID | remove ROOT ID | pack MANIFEST SOURCE SEED_FILE PACKAGE_OUT METADATA_OUT".into()),
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
fn pack(manifest: &str, source: &str, seed: &str, output: &str, metadata: &str) -> Result<()> {
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
    };
    let seed: [u8; 32] = read(seed, 32)?
        .try_into()
        .map_err(|_| "seed must contain exactly 32 raw bytes")?;
    let package = package::sign(&serde_json::to_vec(&payload)?, &seed)?;
    let metadata_value = ReleaseMetadata {
        schema_version: 1,
        host: payload.host,
        plugin_id: payload.plugin_id,
        version: payload.version,
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
