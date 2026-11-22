//! Windows bridge IPC:
//! - Pagefile-backed named shared memory (`CreateFileMappingW` / `MapViewOfFile`, `Local\kfxl-shm-...`)
//! - Named auto-reset Event (`CreateEventW` / `OpenEventW` / `SetEvent` / `WaitForSingleObject`)
//! - Full-duplex overlapped Named Pipe (`\\.\pipe\kfxl-bridge-...`) supporting concurrent reader + writer
//! - Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) so children cannot outlive a crashed host

use std::{
    ffi::c_void,
    io::{self, Read, Write},
    os::windows::io::AsRawHandle,
    process::Child,
    ptr,
    sync::{Arc, atomic::AtomicU32},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
        GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile,
        SYNCHRONIZE, WriteFile,
    },
    System::{
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Memory::{
            CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_BASIC_INFORMATION,
            MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW, PAGE_READWRITE,
            UnmapViewOfFile, VirtualQuery,
        },
        Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_TYPE_BYTE, PIPE_WAIT, PeekNamedPipe,
        },
        Threading::{
            CreateEventW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
        },
    },
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn event_name_for(shm_name: &str) -> String {
    format!("{shm_name}-evt")
}

pub(crate) struct Endpoint {
    shm: String,
    pipe: String,
}

impl Endpoint {
    pub fn new(generation: u64) -> io::Result<Self> {
        let pid = std::process::id();
        Ok(Self {
            shm: format!(r"Local\kfxl-shm-{pid}-{generation}"),
            pipe: format!(r"\\.\pipe\kfxl-bridge-{pid}-{generation}"),
        })
    }

    pub fn shm_name(&self) -> String {
        self.shm.clone()
    }

    pub fn control_addr(&self) -> String {
        self.pipe.clone()
    }
}

pub(crate) struct ShmBacking {
    ptr: *mut u8,
    mapping: HANDLE,
    event: HANDLE,
}

// SAFETY: shared memory mapping + Win32 kernel handles; access discipline is enforced by `Shm`.
unsafe impl Send for ShmBacking {}
unsafe impl Sync for ShmBacking {}

impl ShmBacking {
    pub fn create(name: &str, len: usize) -> io::Result<(Self, *mut u8)> {
        let wname = wide(name);
        let len64 = len as u64;
        let hi = (len64 >> 32) as u32;
        let lo = len64 as u32;
        let mapping = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                ptr::null(),
                PAGE_READWRITE,
                hi,
                lo,
                wname.as_ptr(),
            )
        };
        if mapping.is_null() {
            return Err(io::Error::last_os_error());
        }
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, len) };
        if view.Value.is_null() {
            let err = io::Error::last_os_error();
            unsafe { CloseHandle(mapping) };
            return Err(err);
        }
        let ename = wide(&event_name_for(name));
        let event = unsafe { CreateEventW(ptr::null(), 0, 0, ename.as_ptr()) };
        if event.is_null() {
            let err = io::Error::last_os_error();
            unsafe {
                UnmapViewOfFile(view);
                CloseHandle(mapping);
            }
            return Err(err);
        }
        let ptr = view.Value as *mut u8;
        Ok((
            Self {
                ptr,
                mapping,
                event,
            },
            ptr,
        ))
    }

    pub fn open(name: &str) -> io::Result<(Self, *mut u8, usize)> {
        let wname = wide(name);
        let mapping = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, wname.as_ptr()) };
        if mapping.is_null() {
            return Err(io::Error::last_os_error());
        }
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, 0) };
        if view.Value.is_null() {
            let err = io::Error::last_os_error();
            unsafe { CloseHandle(mapping) };
            return Err(err);
        }
        let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let queried = unsafe {
            VirtualQuery(
                view.Value as *const c_void,
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if queried == 0 || mbi.RegionSize == 0 {
            let err = io::Error::last_os_error();
            unsafe {
                UnmapViewOfFile(view);
                CloseHandle(mapping);
            }
            return Err(err);
        }
        let ename = wide(&event_name_for(name));
        let event = unsafe { OpenEventW(SYNCHRONIZE | EVENT_MODIFY_STATE, 0, ename.as_ptr()) };
        if event.is_null() {
            let err = io::Error::last_os_error();
            unsafe {
                UnmapViewOfFile(view);
                CloseHandle(mapping);
            }
            return Err(err);
        }
        let ptr = view.Value as *mut u8;
        Ok((
            Self {
                ptr,
                mapping,
                event,
            },
            ptr,
            mbi.RegionSize,
        ))
    }

    /// Wake the child after a publish: a single non-blocking `SetEvent` call.
    pub fn wake(&self, _seq: &AtomicU32) {
        unsafe {
            SetEvent(self.event);
        }
    }

    /// Sleep until signaled by `wake()` or `timeout_ms` passes (child side).
    pub fn wait(&self, _seq: &AtomicU32, _seen: u32, timeout_ms: u64) {
        let ms = timeout_ms.min(u32::MAX as u64 - 1) as u32;
        unsafe {
            WaitForSingleObject(self.event, ms);
        }
    }
}

impl Drop for ShmBacking {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.ptr as *mut c_void,
            });
            CloseHandle(self.event);
            CloseHandle(self.mapping);
        }
    }
}

struct PipeHandle(HANDLE);

// SAFETY: overlapped Win32 pipe handle supports concurrent independent `OVERLAPPED` I/O.
unsafe impl Send for PipeHandle {}
unsafe impl Sync for PipeHandle {}

impl Drop for PipeHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

pub(crate) struct ControlListener {
    pipe: Option<PipeHandle>,
    ov: Box<OVERLAPPED>,
    event: HANDLE,
    connected: bool,
}

unsafe impl Send for ControlListener {}

impl ControlListener {
    pub fn bind(addr: &str) -> io::Result<Self> {
        let waddr = wide(addr);
        let handle = unsafe {
            CreateNamedPipeW(
                waddr.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                64 * 1024,
                64 * 1024,
                0,
                ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event.is_null() {
            let err = io::Error::last_os_error();
            unsafe { CloseHandle(handle) };
            return Err(err);
        }
        let mut ov: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
        ov.hEvent = event;
        let ok = unsafe { ConnectNamedPipe(handle, &mut *ov) };
        let connected = if ok != 0 {
            true
        } else {
            let err = unsafe { GetLastError() };
            if err == ERROR_PIPE_CONNECTED {
                true
            } else if err == ERROR_IO_PENDING {
                false
            } else {
                unsafe {
                    CloseHandle(event);
                    CloseHandle(handle);
                }
                return Err(io::Error::from_raw_os_error(err as i32));
            }
        };
        Ok(Self {
            pipe: Some(PipeHandle(handle)),
            ov,
            event,
            connected,
        })
    }

    pub fn accept_step(&mut self, timeout: Duration) -> io::Result<Option<ControlStream>> {
        let Some(pipe) = &self.pipe else {
            return Err(io::Error::other("pipe listener already accepted"));
        };
        if !self.connected {
            let ms = timeout.as_millis().min(u32::MAX as u128 - 1) as u32;
            match unsafe { WaitForSingleObject(self.event, ms) } {
                WAIT_OBJECT_0 => {
                    let mut dummy = 0u32;
                    if unsafe { GetOverlappedResult(pipe.0, &*self.ov, &mut dummy, 0) } == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    self.connected = true;
                }
                WAIT_TIMEOUT => return Ok(None),
                _ => return Err(io::Error::last_os_error()),
            }
        }
        let pipe = self.pipe.take().unwrap();
        Ok(Some(ControlStream {
            pipe: Arc::new(pipe),
            read_timeout: None,
            write_timeout: None,
        }))
    }
}

impl Drop for ControlListener {
    fn drop(&mut self) {
        if let Some(pipe) = &self.pipe
            && !self.connected
        {
            // Cancel the pending overlapped `ConnectNamedPipe` before freeing `self.ov`.
            unsafe {
                CancelIoEx(pipe.0, &*self.ov);
                let mut dummy = 0u32;
                let _ = GetOverlappedResult(pipe.0, &*self.ov, &mut dummy, 1);
            }
        }
        if !self.event.is_null() {
            unsafe {
                CloseHandle(self.event);
            }
        }
    }
}

pub(crate) struct ControlStream {
    pipe: Arc<PipeHandle>,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl ControlStream {
    pub fn connect(addr: &str) -> io::Result<Self> {
        let waddr = wide(addr);
        for _ in 0..50 {
            let handle = unsafe {
                CreateFileW(
                    waddr.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                return Ok(Self {
                    pipe: Arc::new(PipeHandle(handle)),
                    read_timeout: None,
                    write_timeout: None,
                });
            }
            let err = unsafe { GetLastError() };
            if err == ERROR_PIPE_BUSY {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            return Err(io::Error::from_raw_os_error(err as i32));
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "bridge pipe stayed busy",
        ))
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            pipe: self.pipe.clone(),
            read_timeout: self.read_timeout,
            write_timeout: self.write_timeout,
        })
    }

    pub fn set_read_timeout(&mut self, dur: Option<Duration>) -> io::Result<()> {
        self.read_timeout = dur;
        Ok(())
    }

    pub fn set_write_timeout(&mut self, dur: Option<Duration>) -> io::Result<()> {
        self.write_timeout = dur;
        Ok(())
    }

    /// Non-blocking poll used by test modes to detect peer close or a Shutdown message.
    pub fn try_closed(&mut self) -> bool {
        let mut avail = 0u32;
        let ok = unsafe {
            PeekNamedPipe(
                self.pipe.0,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                &mut avail,
                ptr::null_mut(),
            )
        };
        ok == 0 || avail > 0
    }
}

fn finish_overlapped(
    handle: HANDLE,
    ov: &OVERLAPPED,
    timeout: Option<Duration>,
) -> io::Result<usize> {
    let wait_ms = match timeout {
        Some(d) => d.as_millis().min(u32::MAX as u128 - 1) as u32,
        None => INFINITE,
    };
    match unsafe { WaitForSingleObject(ov.hEvent, wait_ms) } {
        WAIT_OBJECT_0 => {
            let mut n = 0u32;
            if unsafe { GetOverlappedResult(handle, ov, &mut n, 0) } == 0 {
                let err = unsafe { GetLastError() };
                if err == ERROR_BROKEN_PIPE {
                    return Ok(0);
                }
                return Err(io::Error::from_raw_os_error(err as i32));
            }
            Ok(n as usize)
        }
        WAIT_TIMEOUT => {
            unsafe {
                CancelIoEx(handle, ov);
                let mut dummy = 0u32;
                let _ = GetOverlappedResult(handle, ov, &mut dummy, 1);
            }
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "bridge pipe timed out",
            ))
        }
        _ => Err(io::Error::last_os_error()),
    }
}

struct LocalEvent(HANDLE);
impl LocalEvent {
    fn new() -> io::Result<Self> {
        let h = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if h.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(h))
        }
    }
}
impl Drop for LocalEvent {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

impl Read for ControlStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let evt = LocalEvent::new()?;
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = evt.0;
        let to_read = buf.len().min(u32::MAX as usize) as u32;
        let mut n = 0u32;
        let ok = unsafe { ReadFile(self.pipe.0, buf.as_mut_ptr(), to_read, &mut n, &mut ov) };
        if ok != 0 {
            return Ok(n as usize);
        }
        let err = unsafe { GetLastError() };
        if err == ERROR_IO_PENDING {
            finish_overlapped(self.pipe.0, &ov, self.read_timeout)
        } else if err == ERROR_BROKEN_PIPE {
            Ok(0)
        } else {
            Err(io::Error::from_raw_os_error(err as i32))
        }
    }
}

impl Write for ControlStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let evt = LocalEvent::new()?;
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = evt.0;
        let to_write = buf.len().min(u32::MAX as usize) as u32;
        let mut n = 0u32;
        let ok = unsafe { WriteFile(self.pipe.0, buf.as_ptr(), to_write, &mut n, &mut ov) };
        if ok != 0 {
            return Ok(n as usize);
        }
        let err = unsafe { GetLastError() };
        if err == ERROR_IO_PENDING {
            finish_overlapped(self.pipe.0, &ov, self.write_timeout)
        } else {
            Err(io::Error::from_raw_os_error(err as i32))
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Keeps a child process bound to a Windows Job Object with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so children die automatically if the
/// host exits or crashes.
pub(crate) struct ChildGuard {
    job: HANDLE,
}

unsafe impl Send for ChildGuard {}

impl ChildGuard {
    pub fn attach(child: &Child) -> Self {
        let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if job.is_null() {
            return Self {
                job: ptr::null_mut(),
            };
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            unsafe { CloseHandle(job) };
            return Self {
                job: ptr::null_mut(),
            };
        }
        let assigned = unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) };
        if assigned == 0 {
            // Nested job assignment can fail on older/restricted environments; keep running.
            unsafe { CloseHandle(job) };
            return Self {
                job: ptr::null_mut(),
            };
        }
        Self { job }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.job.is_null() {
            unsafe {
                CloseHandle(self.job);
            }
        }
    }
}
