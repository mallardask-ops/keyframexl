//! OS-specific IPC primitives for the out-of-process plugin bridge:
//! - Shared memory mapping + audio-block wake signal (`ShmBacking`)
//! - Bidirectional framed control channel (`ControlListener`, `ControlStream`)
//! - Session endpoint naming (`Endpoint`)
//! - Child process lifetime guard (`ChildGuard`)

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(crate) use windows::*;
