//! Linux bridge IPC: `/dev/shm` `mmap` + `SYS_futex` + Unix domain socket.

use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::fd::AsRawFd,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::Child,
    sync::atomic::AtomicU32,
    time::Duration,
};

pub(crate) struct Endpoint {
    dir: tempfile::TempDir,
}

impl Endpoint {
    pub fn new(_generation: u64) -> io::Result<Self> {
        let base = Path::new("/dev/shm");
        let dir = if base.is_dir() {
            tempfile::Builder::new()
                .prefix("kfxl-bridge-")
                .tempdir_in(base)?
        } else {
            tempfile::tempdir()?
        };
        Ok(Self { dir })
    }

    pub fn shm_name(&self) -> String {
        self.dir.path().join("audio.blk").display().to_string()
    }

    pub fn control_addr(&self) -> String {
        self.dir.path().join("control.sock").display().to_string()
    }
}

pub(crate) struct ShmBacking {
    ptr: *mut u8,
    len: usize,
    _file: File,
}

// SAFETY: shared memory mapping; access discipline is enforced by `Shm`.
unsafe impl Send for ShmBacking {}
unsafe impl Sync for ShmBacking {}

fn mmap_file(file: &File, len: usize) -> io::Result<*mut u8> {
    // SAFETY: fresh shared mapping of a file we own, of its full length.
    let p = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            file.as_raw_fd(),
            0,
        )
    };
    if p == libc::MAP_FAILED {
        Err(io::Error::last_os_error())
    } else {
        Ok(p as *mut u8)
    }
}

impl ShmBacking {
    pub fn create(name: &str, len: usize) -> io::Result<(Self, *mut u8)> {
        let path = PathBuf::from(name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.set_len(len as u64)?;
        let ptr = mmap_file(&file, len)?;
        Ok((
            Self {
                ptr,
                len,
                _file: file,
            },
            ptr,
        ))
    }

    pub fn open(name: &str) -> io::Result<(Self, *mut u8, usize)> {
        let path = PathBuf::from(name);
        let file = OpenOptions::new().read(true).write(true).open(&path)?;
        let len = file.metadata()?.len() as usize;
        let ptr = mmap_file(&file, len)?;
        Ok((
            Self {
                ptr,
                len,
                _file: file,
            },
            ptr,
            len,
        ))
    }

    /// Wake the child after a publish: a single non-blocking syscall (audio side).
    pub fn wake(&self, seq: &AtomicU32) {
        // SAFETY: shared (non-private) futex on a 4-byte aligned word in the mapping.
        unsafe { libc::syscall(libc::SYS_futex, seq.as_ptr(), libc::FUTEX_WAKE, 1) };
    }

    /// Sleep until `seq` differs from `seen` or `timeout_ms` passes (child side).
    pub fn wait(&self, seq: &AtomicU32, seen: u32, timeout_ms: u64) {
        let ts = libc::timespec {
            tv_sec: (timeout_ms / 1000) as libc::time_t,
            tv_nsec: ((timeout_ms % 1000) * 1_000_000) as libc::c_long,
        };
        // SAFETY: shared (non-private) futex on a 4-byte aligned word in the mapping.
        unsafe {
            libc::syscall(
                libc::SYS_futex,
                seq.as_ptr(),
                libc::FUTEX_WAIT,
                seen,
                &ts as *const libc::timespec,
            )
        };
    }
}

impl Drop for ShmBacking {
    fn drop(&mut self) {
        // SAFETY: unmapping our own mapping once.
        unsafe { libc::munmap(self.ptr as *mut _, self.len) };
    }
}

pub(crate) struct ControlListener {
    listener: UnixListener,
}

impl ControlListener {
    pub fn bind(addr: &str) -> io::Result<Self> {
        let listener = UnixListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener })
    }

    pub fn accept_step(&mut self, timeout: Duration) -> io::Result<Option<ControlStream>> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                Ok(Some(ControlStream { stream }))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(timeout);
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

pub(crate) struct ControlStream {
    stream: UnixStream,
}

impl ControlStream {
    pub fn connect(addr: &str) -> io::Result<Self> {
        Ok(Self {
            stream: UnixStream::connect(addr)?,
        })
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            stream: self.stream.try_clone()?,
        })
    }

    pub fn set_read_timeout(&mut self, dur: Option<Duration>) -> io::Result<()> {
        self.stream.set_read_timeout(dur)
    }

    pub fn set_write_timeout(&mut self, dur: Option<Duration>) -> io::Result<()> {
        self.stream.set_write_timeout(dur)
    }

    /// Non-blocking poll used by test modes to detect peer close or a Shutdown message.
    pub fn try_closed(&mut self) -> bool {
        let _ = self.stream.set_nonblocking(true);
        let mut buf = [0u8; 4];
        match self.stream.read(&mut buf) {
            Ok(0) => true,
            Ok(_) => true,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => false,
            Err(_) => true,
        }
    }
}

impl Read for ControlStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for ControlStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

pub(crate) struct ChildGuard;

impl ChildGuard {
    pub fn attach(_child: &Child) -> Self {
        Self
    }
}
