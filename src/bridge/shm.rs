//! The audio block shared with a plugin-host child: one mapping per instance.
//!
//! Layout (all little-endian, `#[repr(C)]`, offsets 64-byte aligned):
//!   Header                     (atomics: the only fields both sides touch at once)
//!   events  [RawEvent; max_events]
//!   input   [f32; max_frames * 2]   interleaved stereo, parent → child
//!   key     [f32; max_frames * 2]   the sidechain, parent → child (when `keyed`)
//!   output  [f32; max_frames * 2]   interleaved stereo, child → parent
//!   extra   [f32; max_frames * 2 * MAX_EXTRA_PAIRS]  a multi-out plugin's
//!           other pairs, one lane of `max_frames * 2` each, child → parent
//!
//! Ownership is a hand-off by sequence numbers: the parent writes the block
//! (events, input, frames, transport) only while `out_seq == in_seq` (the child
//! is idle), then publishes `in_seq + 1` with Release. The child reads the block
//! after an Acquire load of `in_seq`, writes `output`, and publishes `out_seq`
//! with Release; the parent reads `output` after an Acquire load of `out_seq`.
//! So the plain (non-atomic) buffers always have a single owner.

use super::ipc::ShmBacking;
use std::{
    io,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};

const MAGIC: u32 = 0x4b46_4252; // "KFBR"
pub(crate) const LAYOUT_VERSION: u32 = 4;

/// Block command in `Header::cmd`.
pub(crate) const CMD_PROCESS: u32 = 0;
pub(crate) const CMD_PREPARE_OFFLINE: u32 = 1;
pub(crate) const CMD_FINISH_OFFLINE: u32 = 2;

#[repr(C)]
pub(crate) struct Header {
    magic: u32,
    version: u32,
    pub max_frames: u32,
    pub max_events: u32,
    /// Last block the parent published (the child waits on it).
    pub in_seq: AtomicU32,
    /// Last block the child finished.
    pub out_seq: AtomicU32,
    pub frames: AtomicU32,
    pub n_events: AtomicU32,
    pub rate: AtomicU32,
    pub playing: AtomicU32,
    /// Extra output pairs the child wrote for the last block (set before `out_seq`).
    pub pairs: AtomicU32,
    /// The hosted plugin's own latency in frames, as of the last block.
    pub latency: AtomicU32,
    /// 1: the block has a sidechain key (`key`), for the plugin's aux input.
    pub keyed: AtomicU32,
    /// 1: offline export in progress (child switches to blocking lock & offline mode).
    pub offline: AtomicU32,
    /// Command for the next published sequence (`CMD_*`).
    pub cmd: AtomicU32,
    pub tempo_bits: AtomicU64,
    pub frame: AtomicU64,
}

/// A note or controller at a frame of the block (kind 0 on, 1 off, 2 raw MIDI).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct RawEvent {
    pub kind: u8,
    pub b: [u8; 3],
    pub at: u32,
}

const fn align64(n: usize) -> usize {
    (n + 63) & !63
}

struct Offsets {
    events: usize,
    input: usize,
    key: usize,
    output: usize,
    extra: usize,
    len: usize,
}

fn offsets(max_frames: u32, max_events: u32) -> Offsets {
    let events = align64(std::mem::size_of::<Header>());
    let input = align64(events + max_events as usize * std::mem::size_of::<RawEvent>());
    let key = align64(input + max_frames as usize * 2 * 4);
    let output = align64(key + max_frames as usize * 2 * 4);
    let extra = align64(output + max_frames as usize * 2 * 4);
    let len = align64(extra + max_frames as usize * 2 * 4 * keyframexl::plugin::MAX_EXTRA_PAIRS);
    Offsets {
        events,
        input,
        key,
        output,
        extra,
        len,
    }
}

/// One mapping of the shared block (each process has its own).
pub(crate) struct Shm {
    ptr: *mut u8,
    off: Offsets,
    backing: ShmBacking,
}

// SAFETY: the mapping is plain shared memory; access discipline is the
// sequence hand-off described above, and the header fields are atomics.
unsafe impl Send for Shm {}
unsafe impl Sync for Shm {}

impl Shm {
    /// Parent side: a new zeroed shared block named `name`.
    pub fn create(name: &str, max_frames: u32, max_events: u32) -> io::Result<Self> {
        let off = offsets(max_frames, max_events);
        let (backing, ptr) = ShmBacking::create(name, off.len)?;
        // SAFETY: the mapping is at least `size_of::<Header>()` bytes, page aligned, zeroed.
        unsafe {
            let h = ptr as *mut Header;
            (*h).magic = MAGIC;
            (*h).version = LAYOUT_VERSION;
            (*h).max_frames = max_frames;
            (*h).max_events = max_events;
        }
        Ok(Self { ptr, off, backing })
    }

    /// Child side: map an existing block and validate its header and size.
    pub fn open(name: &str) -> io::Result<Self> {
        let (backing, ptr, mapped_len) = ShmBacking::open(name)?;
        if mapped_len < std::mem::size_of::<Header>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bridge block too short",
            ));
        }
        // SAFETY: mapped and at least a header long.
        let (magic, version, mf, me) = unsafe {
            let h = &*(ptr as *const Header);
            (h.magic, h.version, h.max_frames, h.max_events)
        };
        let off = offsets(mf, me);
        // On Windows `MapViewOfFile` rounds RegionSize up to the page granularity,
        // so `mapped_len >= off.len` is valid.
        if magic != MAGIC || version != LAYOUT_VERSION || mapped_len < off.len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bridge block layout mismatch",
            ));
        }
        Ok(Self { ptr, off, backing })
    }

    pub fn header(&self) -> &Header {
        // SAFETY: the mapping starts with a Header and lives as long as self.
        unsafe { &*(self.ptr as *const Header) }
    }

    pub fn max_frames(&self) -> usize {
        self.header().max_frames as usize
    }

    pub fn max_events(&self) -> usize {
        self.header().max_events as usize
    }

    /// Wake the child after publishing `in_seq` (parent audio thread).
    pub fn wake_child(&self) {
        self.backing.wake(&self.header().in_seq);
    }

    /// Sleep until a new block is published or `timeout_ms` elapses (child side).
    pub fn wait_for_block(&self, seen: u32, timeout_ms: u64) {
        self.backing.wait(&self.header().in_seq, seen, timeout_ms);
    }

    /// The events area. Callers follow the hand-off: only the current owner touches it.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn events(&self) -> &mut [RawEvent] {
        // SAFETY: in bounds by `offsets`; exclusive by the hand-off protocol.
        unsafe {
            std::slice::from_raw_parts_mut(
                self.ptr.add(self.off.events) as *mut RawEvent,
                self.max_events(),
            )
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn input(&self) -> &mut [f32] {
        // SAFETY: as for `events`.
        unsafe {
            std::slice::from_raw_parts_mut(
                self.ptr.add(self.off.input) as *mut f32,
                self.max_frames() * 2,
            )
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn key(&self) -> &mut [f32] {
        // SAFETY: as for `events`.
        unsafe {
            std::slice::from_raw_parts_mut(
                self.ptr.add(self.off.key) as *mut f32,
                self.max_frames() * 2,
            )
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn output(&self) -> &mut [f32] {
        // SAFETY: as for `events`.
        unsafe {
            std::slice::from_raw_parts_mut(
                self.ptr.add(self.off.output) as *mut f32,
                self.max_frames() * 2,
            )
        }
    }

    /// The extra output lanes, `max_frames * 2` apart (see the layout above).
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn extra(&self) -> &mut [f32] {
        // SAFETY: as for `events`.
        unsafe {
            std::slice::from_raw_parts_mut(
                self.ptr.add(self.off.extra) as *mut f32,
                self.max_frames() * 2 * keyframexl::plugin::MAX_EXTRA_PAIRS,
            )
        }
    }
}

pub(crate) fn tempo_of(h: &Header) -> f64 {
    f64::from_bits(h.tempo_bits.load(Ordering::Relaxed))
}
