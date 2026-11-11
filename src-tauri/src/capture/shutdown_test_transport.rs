//! Real local control transports for the cleanup contract, including Windows.
use crate::models::core::CoreEndpoint;

#[cfg(unix)]
pub(super) type Listener = std::os::unix::net::UnixListener;
#[cfg(windows)]
pub(super) type Listener = std::fs::File;

pub(super) fn listen() -> (Listener, CoreEndpoint, tempfile::TempDir) {
    #[cfg(unix)]
    {
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let path = dir.path().join("cleanup.sock");
        let listener = Listener::bind(&path).unwrap();
        (
            listener,
            CoreEndpoint {
                transport: "unix-socket".into(),
                path: path.to_string_lossy().into_owned(),
            },
            dir,
        )
    }
    #[cfg(windows)]
    {
        use std::os::windows::{ffi::OsStrExt, io::FromRawHandle};
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            Storage::FileSystem::PIPE_ACCESS_DUPLEX,
            System::Pipes::{CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT},
        };
        let dir = tempfile::tempdir().unwrap();
        let path = format!(
            r"\\.\pipe\znet-cleanup-{}-{}",
            std::process::id(),
            dir.path().file_name().unwrap().to_string_lossy()
        );
        let name: Vec<u16> = std::ffi::OsStr::new(&path)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                4096,
                4096,
                5000,
                std::ptr::null(),
            )
        };
        assert_ne!(
            handle,
            INVALID_HANDLE_VALUE,
            "{}",
            std::io::Error::last_os_error()
        );
        let listener = unsafe { std::fs::File::from_raw_handle(handle) };
        (
            listener,
            CoreEndpoint {
                transport: "named-pipe".into(),
                path,
            },
            dir,
        )
    }
}

#[cfg(unix)]
pub(super) fn accept(listener: Listener) -> std::os::unix::net::UnixStream {
    listener.accept().unwrap().0
}

#[cfg(windows)]
pub(super) fn accept(listener: Listener) -> std::fs::File {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{Foundation::ERROR_PIPE_CONNECTED, System::Pipes::ConnectNamedPipe};
    let connected = unsafe { ConnectNamedPipe(listener.as_raw_handle(), std::ptr::null_mut()) };
    if connected == 0 {
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(ERROR_PIPE_CONNECTED as i32)
        );
    }
    listener
}
