use std::{
    collections::BTreeSet,
    fs::File,
    io::Read,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_plugin_sandbox::{
    contract::{Component, Request, MAX_MANIFEST_BYTES, MAX_SOURCE_BYTES},
    policy::Authority,
    runtime::{execute, Summary},
};
fn bounded(path: &str, limit: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("input exceeds limit".into());
    }
    Ok(bytes)
}
fn main() {
    if let Err(error) = run() {
        eprintln!("plugin lab: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: znet-plugin-lab MANIFEST SOURCE GRANTS_JSON SUMMARY_JSON (local development only)".into());
    }
    let manifest = bounded(&args[0], MAX_MANIFEST_BYTES)?;
    let source = String::from_utf8(bounded(&args[1], MAX_SOURCE_BYTES)?)?;
    let component = Component::load(&manifest, &source)?;
    let grants: BTreeSet<Request> = serde_json::from_slice(&bounded(&args[2], 4096)?)?;
    let summary: Summary = serde_json::from_slice(&bounded(&args[3], 4096)?)?;
    // Fixed lab admission ceiling, independent of guest declarations.
    let ceiling = serde_json::from_str(
        r#"[{"capability":"plugin.self.read","scope":"self"},{"capability":"records.summary.read","scope":"selection:demo"}]"#,
    )?;
    let authority = Authority::admit(&component, &ceiling)?;
    authority.authorize(grants, Duration::from_secs(60))?;
    let result = execute(
        &component,
        &authority,
        Some(("demo".into(), summary)),
        Arc::new(AtomicBool::new(false)),
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
