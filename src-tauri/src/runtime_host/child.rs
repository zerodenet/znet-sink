//! Each Windows kernel generation is created inside its own private job.
use std::{io, process::Command};

#[cfg(not(windows))]
pub(super) type ManagedChild = std::process::Child;
#[cfg(windows)]
#[path = "windows_job.rs"]
mod job;
#[cfg(windows)]
#[path = "windows_spawn.rs"]
mod windows;
#[cfg(windows)]
pub(super) use windows::ManagedChild;

/// The Windows creation attributes install containment before any child code
/// can run. There is no spawn-then-assign gap or uncontained fallback.
pub(super) fn spawn(command: &mut Command) -> io::Result<ManagedChild> {
    #[cfg(not(windows))]
    {
        command.spawn()
    }
    #[cfg(windows)]
    {
        windows::spawn(command)
    }
}

#[cfg(all(test, windows))]
#[path = "windows_job_tests.rs"]
mod tests;
