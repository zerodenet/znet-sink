//! Keep desktop framework resources out of managed kernel processes.
use std::process::Command;

pub(crate) fn command(program: &str) -> Command {
    let command = super::common::background_command(program);
    #[cfg(unix)]
    let command = {
        let mut command = command;
        use std::os::unix::process::CommandExt;
        // SAFETY: the child callback uses only descriptor/resource-limit
        // syscalls and stack values. It must never allocate, log or take locks.
        unsafe { command.pre_exec(prepare_child) };
        command
    };
    command
}

#[cfg(unix)]
fn requested_limit(current: libc::rlim_t, hard: libc::rlim_t) -> libc::rlim_t {
    current.max(4096).min(hard)
}

#[cfg(unix)]
fn prepare_child() -> std::io::Result<()> {
    use std::io;
    // SAFETY: limits points to writable memory with the ABI-required layout.
    let mut limits = std::mem::MaybeUninit::<libc::rlimit>::uninit();
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, limits.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut limits = unsafe { limits.assume_init() };
    mark_inherited_descriptors(limits.rlim_cur)?;
    let requested = requested_limit(limits.rlim_cur, limits.rlim_max);
    if requested != limits.rlim_cur {
        limits.rlim_cur = requested;
        // This changes only the child soft limit; the desktop and system-wide
        // limits remain intact, and an administrator's hard limit is respected.
        if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limits) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn mark_inherited_descriptors(ceiling: libc::rlim_t) -> std::io::Result<()> {
    // Mark, rather than close: Rust's internal exec-error pipe must stay usable
    // until exec succeeds, otherwise a missing executable can look successful.
    #[cfg(target_os = "macos")]
    {
        // proc_pidinfo is a direct proc_info syscall wrapper. Use a fixed
        // stack buffer: no allocation/locks after fork, and no scan through
        // a million unused slots when a developer shell has a high limit.
        let mut descriptors = [libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0,
        }; 1024];
        let capacity = std::mem::size_of_val(&descriptors);
        let bytes = unsafe {
            libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDLISTFDS,
                0,
                descriptors.as_mut_ptr().cast(),
                capacity as i32,
            )
        };
        let entry_size = std::mem::size_of::<libc::proc_fdinfo>();
        if bytes > 0 && (bytes as usize) < capacity && (bytes as usize) % entry_size == 0 {
            for descriptor in &descriptors[..bytes as usize / entry_size] {
                if descriptor.proc_fd >= 3 {
                    mark_close_on_exec(descriptor.proc_fd)?;
                }
            }
            return Ok(());
        }
        // A full buffer may be truncated. Fall back without losing entries.
    }
    #[cfg(target_os = "linux")]
    if unsafe {
        libc::syscall(
            libc::SYS_close_range,
            3u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    } == 0
    {
        return Ok(());
    }
    let ceiling = if ceiling == libc::RLIM_INFINITY {
        let open_max = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
        if open_max < 0 {
            return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
        }
        open_max as libc::rlim_t
    } else {
        ceiling
    };
    for fd in 3..ceiling.min(i32::MAX as libc::rlim_t) as i32 {
        mark_close_on_exec(fd)?;
    }
    Ok(())
}

#[cfg(unix)]
fn mark_close_on_exec(fd: i32) -> std::io::Result<()> {
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EBADF) {
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "kernel_command_tests.rs"]
mod tests;
