//! Atomic Windows process/job creation, with an explicit inherited-handle list.
use super::job::Job;
use std::{
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    process::{ChildStderr, ChildStdin, Command, ExitStatus},
};
use windows_sys::Win32::{
    Foundation::{
        SetHandleInformation, GENERIC_WRITE, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
    System::{
        Pipes::CreatePipe,
        Threading::{
            CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
            InitializeProcThreadAttributeList, UpdateProcThreadAttribute, WaitForSingleObject,
            CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROC_THREAD_ATTRIBUTE_JOB_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW,
        },
    },
};

#[path = "windows_spawn/commandline.rs"]
mod commandline;
#[path = "windows_spawn/environment.rs"]
mod environment;

pub(crate) struct ManagedChild {
    process: OwnedHandle,
    pid: u32,
    status: Option<ExitStatus>,
    job: Job,
    pub(crate) stdin: Option<ChildStdin>,
    pub(crate) stderr: Option<ChildStderr>,
}

impl ManagedChild {
    pub(crate) fn id(&self) -> u32 {
        self.pid
    }
    fn poll_exit(&mut self, timeout_ms: u32) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), timeout_ms) } {
            WAIT_TIMEOUT => return Ok(None),
            WAIT_OBJECT_0 => {}
            _ => return Err(io::Error::last_os_error()),
        }
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let status = ExitStatus::from_raw(code);
        self.status = Some(status);
        Ok(Some(status))
    }
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let Some(status) = self.poll_exit(0)? else {
            return Ok(None);
        };
        if !self.job.retire_remaining()? {
            return Ok(None);
        }
        Ok(Some(status))
    }
    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        let status = self.poll_exit(5000)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "old kernel process exit remains unconfirmed",
            )
        })?;
        self.job.wait_empty()?;
        Ok(status)
    }
    pub(crate) fn kill(&mut self) -> io::Result<()> {
        self.job.terminate()
    }
}

struct Attributes(Vec<u128>);
impl Attributes {
    fn new() -> io::Result<Self> {
        let mut bytes = 0;
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0u128; bytes.div_ceil(std::mem::size_of::<u128>())];
        if unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 2, 0, &mut bytes)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(storage))
    }
    fn raw(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_mut_ptr().cast()
    }
    fn handles(&mut self, attribute: u32, handles: &[HANDLE]) -> io::Result<()> {
        if unsafe {
            UpdateProcThreadAttribute(
                self.raw(),
                0,
                attribute as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(handles),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.raw()) };
    }
}

fn pipe(parent_writes: bool) -> io::Result<(OwnedHandle, OwnedHandle)> {
    let security = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (std::ptr::null_mut(), std::ptr::null_mut());
    if unsafe { CreatePipe(&mut read, &mut write, &security, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let read = unsafe { OwnedHandle::from_raw_handle(read) };
    let write = unsafe { OwnedHandle::from_raw_handle(write) };
    let (parent, child) = if parent_writes {
        (write, read)
    } else {
        (read, write)
    };
    if unsafe { SetHandleInformation(parent.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((parent, child))
}

fn wide(value: &std::ffi::OsStr) -> io::Result<Vec<u16>> {
    let mut encoded: Vec<_> = value.encode_wide().collect();
    if encoded.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process argument contains NUL",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

pub(super) fn spawn(command: &Command) -> io::Result<ManagedChild> {
    let program = wide(command.get_program())?;
    let mut line = commandline::encode(
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|arg| arg.encode_wide().collect::<Vec<_>>()),
    )?;
    let environment = environment::build(command)?;
    let directory = command
        .get_current_dir()
        .map(|path| wide(path.as_os_str()))
        .transpose()?;
    let job = Job::new()?;
    let (stdin, child_stdin) = pipe(true)?;
    let (stderr, child_stderr) = pipe(false)?;
    let security = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let nul = unsafe {
        CreateFileW(
            [b'N' as u16, b'U' as u16, b'L' as u16, 0].as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &security,
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if nul == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let nul = unsafe { OwnedHandle::from_raw_handle(nul) };
    let handles = [
        child_stdin.as_raw_handle(),
        child_stderr.as_raw_handle(),
        nul.as_raw_handle(),
    ];
    let jobs = [job.raw()];
    let mut attributes = Attributes::new()?;
    attributes.handles(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
    attributes.handles(PROC_THREAD_ATTRIBUTE_JOB_LIST, &jobs)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = std::mem::size_of_val(&startup) as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdError = handles[1];
    startup.StartupInfo.hStdOutput = handles[2];
    startup.lpAttributeList = attributes.raw();
    let mut process = PROCESS_INFORMATION::default();
    if unsafe {
        CreateProcessW(
            program.as_ptr(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_ptr().cast(),
            directory.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
            &startup.StartupInfo,
            &mut process,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
    let _thread = unsafe { OwnedHandle::from_raw_handle(process.hThread) };
    // Attribute value arrays outlive the actual attribute list; no handle is
    // made inheritable except the three explicitly selected standard handles.
    drop(attributes);
    Ok(ManagedChild {
        process: handle,
        pid: process.dwProcessId,
        status: None,
        job,
        stdin: Some(stdin.into()),
        stderr: Some(stderr.into()),
    })
}
