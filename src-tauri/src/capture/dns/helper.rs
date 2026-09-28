//! A lifetime pipe ensures DNS cleanup also runs if the GUI is force-killed.
use std::{
    io::{self, Write},
    sync::mpsc,
    time::Duration,
};
#[path = "helper/native.rs"]
mod native;

pub(super) fn run_if_requested() -> Option<io::Result<()>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("__tun-dns-helper") {
        return None;
    }
    Some((|| {
        if args.len() != 4 {
            return Err(io::Error::other("invalid DNS guardian arguments"));
        }
        let pid: u32 = args[3].parse().map_err(io::Error::other)?;
        super::validate_target(&args[1], &args[2], pid)?;
        let owner = native::Owner::new(pid)?;
        let guard = native::Resolver::install(&args[1], &args[2], pid);
        let response = guard.as_ref().map(|_| ()).map_err(|e| e.to_string());
        serde_json::to_writer(io::stdout(), &response).map_err(io::Error::other)?;
        println!();
        io::stdout().flush()?;
        let mut guard = guard?;
        let (closed, closing) = mpsc::channel();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut byte = [0u8; 1];
            while matches!(io::stdin().read(&mut byte), Ok(1)) {}
            let _ = closed.send(());
        });
        loop {
            if closing.recv_timeout(Duration::from_secs(1)).is_ok() || !owner.alive() {
                break;
            }
        }
        guard.remove()
    })())
}
