//! Platform abstraction layer (`docs/v0.2.1.md`, W0/W1).
//!
//! In W0 this hosts clean non-Linux stubs for the Linux-only FL/Wine modules
//! and temporary stubs for plugin hosting / IPC bridge until W3 and W4 land.

#[cfg(not(target_os = "linux"))]
pub mod stubs;
