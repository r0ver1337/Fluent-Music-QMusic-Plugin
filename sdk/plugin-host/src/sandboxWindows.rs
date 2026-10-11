#![allow(non_snake_case)]
use crate::sandboxProfile::{grantRead, profile};
use std::{
    fs::File,
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    Storage::FileSystem::*,
    System::{JobObjects::*, Pipes::*, Threading::*},
};

pub struct WorkerChild {
    Process: OwnedHandle,
    Job: OwnedHandle,
}
impl WorkerChild {
    pub fn kill(&mut self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.Job.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn wait(&mut self) -> io::Result<()> {
        if unsafe { WaitForSingleObject(self.Process.as_raw_handle(), 5000) } == WAIT_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
impl Drop for WorkerChild {
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.wait();
    }
}
fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}
fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let mut read = std::ptr::null_mut();
    let mut write = std::ptr::null_mut();
    if unsafe { CreatePipe(&mut read, &mut write, &security, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((owned(read)?, owned(write)?))
}
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in text.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            out.extend(std::iter::repeat_n('\\', slashes));
        }
        slashes = 0;
        out.push(c);
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}
pub fn spawn(
    worker: &Path,
    plugin: &Path,
    id: &str,
    arguments: &[String],
    sandbox: bool,
) -> io::Result<(WorkerChild, File, File)> {
    let sid = if sandbox {
        let sid = profile(plugin, id)?;
        grantRead(plugin, sid.0, true)?;
        grantRead(worker, sid.0, false)?;
        Some(sid)
    } else {
        None
    };
    let security = SECURITY_CAPABILITIES {
        AppContainerSid: sid.as_ref().map_or(std::ptr::null_mut(), |s| s.0),
        Capabilities: std::ptr::null_mut(),
        CapabilityCount: 0,
        Reserved: 0,
    };
    let (inputRead, inputWrite) = pipe()?;
    let (outputRead, outputWrite) = pipe()?;
    unsafe {
        SetHandleInformation(inputWrite.as_raw_handle(), HANDLE_FLAG_INHERIT, 0);
        SetHandleInformation(outputRead.as_raw_handle(), HANDLE_FLAG_INHERIT, 0);
    }
    let nullName = wide(std::ffi::OsStr::new("NUL"));
    let securityAttributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let stderr = owned(unsafe {
        CreateFileW(
            nullName.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &securityAttributes,
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    })?;
    let handles = [
        inputRead.as_raw_handle(),
        outputWrite.as_raw_handle(),
        stderr.as_raw_handle(),
    ];
    let mut length = 0;
    let count = if sandbox { 3 } else { 1 };
    unsafe {
        InitializeProcThreadAttributeList(std::ptr::null_mut(), count, 0, &mut length);
    }
    let mut buffer = vec![0usize; length.div_ceil(size_of::<usize>())];
    let attributes = buffer.as_mut_ptr().cast();
    if unsafe { InitializeProcThreadAttributeList(attributes, count, 0, &mut length) } == 0 {
        return Err(io::Error::last_os_error());
    }
    struct Attributes(windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST);
    impl Drop for Attributes {
        fn drop(&mut self) {
            unsafe {
                DeleteProcThreadAttributeList(self.0);
            }
        }
    }
    let _attributes = Attributes(attributes);
    if unsafe {
        UpdateProcThreadAttribute(
            attributes,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            handles.as_ptr().cast(),
            std::mem::size_of_val(&handles),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if sandbox
        && unsafe {
            UpdateProcThreadAttribute(
                attributes,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                (&security as *const SECURITY_CAPABILITIES).cast(),
                size_of::<SECURITY_CAPABILITIES>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let win32kLockdown = 1u64 << 28;
    if sandbox
        && unsafe {
            UpdateProcThreadAttribute(
                attributes,
                0,
                PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY as usize,
                (&win32kLockdown as *const u64).cast(),
                size_of::<u64>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes;
    let mut process = PROCESS_INFORMATION::default();
    let command = std::iter::once(worker.to_string_lossy().into_owned())
        .chain(arguments.iter().cloned())
        .map(|s| quote(&s))
        .collect::<Vec<_>>()
        .join(" ");
    let mut command = wide(std::ffi::OsStr::new(&command));
    let cwd = wide(plugin.as_os_str());
    let workerName = wide(worker.as_os_str());
    let mut environment = Vec::new();
    for name in [
        "APPDATA",
        "LOCALAPPDATA",
        "SystemDrive",
        "SystemRoot",
        "TEMP",
        "TMP",
        "USERDOMAIN",
        "USERNAME",
        "USERPROFILE",
        "WINDIR",
    ] {
        if let Some(value) = std::env::var_os(name) {
            environment.extend(wide(std::ffi::OsStr::new(&format!(
                "{name}={}",
                value.to_string_lossy()
            ))));
        }
    }
    environment.push(0);
    if unsafe {
        CreateProcessW(
            workerName.as_ptr(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT
                | CREATE_NO_WINDOW
                | CREATE_SUSPENDED
                | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr().cast(),
            cwd.as_ptr(),
            &startup.StartupInfo,
            &mut process,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let processHandle = owned(process.hProcess)?;
    let thread = owned(process.hThread)?;
    let job = owned(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) })?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | if sandbox {
            JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        } else {
            0
        };
    limits.BasicLimitInformation.ActiveProcessLimit = 1;
    let setup = unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) != 0
            && AssignProcessToJobObject(job.as_raw_handle(), processHandle.as_raw_handle()) != 0
    };
    if !setup {
        unsafe {
            TerminateProcess(processHandle.as_raw_handle(), 1);
        }
        return Err(io::Error::last_os_error());
    }
    let child = WorkerChild {
        Process: processHandle,
        Job: job,
    };
    if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
        return Err(io::Error::last_os_error());
    }
    drop(inputRead);
    drop(outputWrite);
    Ok((child, inputWrite.into(), outputRead.into()))
}
pub fn verifySandbox() -> io::Result<()> {
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = owned(token)?;
    let mut value = 0u32;
    let mut length = 0;
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenIsAppContainer,
            (&mut value as *mut u32).cast(),
            4,
            &mut length,
        )
    } == 0
        || value != 1
    {
        return Err(io::Error::other("Worker is not an AppContainer"));
    }
    let mut capabilities = [0u64; 128];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenCapabilities,
            capabilities.as_mut_ptr().cast(),
            std::mem::size_of_val(&capabilities) as u32,
            &mut length,
        )
    } == 0
        || unsafe { *capabilities.as_ptr().cast::<u32>() } != 0
    {
        return Err(io::Error::other(
            "Worker must not have direct network capabilities",
        ));
    }
    Ok(())
}
