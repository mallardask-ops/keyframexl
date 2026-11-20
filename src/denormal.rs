//! Flush-to-zero for DSP threads.
//!
//! Reverb, filter and delay tails decay into subnormal floats, which x86 and
//! ARM handle in microcode at up to ~100× the normal cost: a quiet project
//! spikes the CPU right after the sound stops. FTZ/DAZ make the FPU treat
//! them as zero. The flags are per-thread, so every DSP entry point calls
//! [`flush_to_zero`]; it costs a register read once the flags are set.

/// Enable flush-to-zero (and denormals-are-zero on x86) for this thread.
#[inline]
pub fn flush_to_zero() {
    #[cfg(target_arch = "x86_64")]
    #[allow(deprecated)]
    {
        use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};
        // FTZ = bit 15, DAZ = bit 6. Every x86_64 CPU has SSE2 and DAZ.
        const FLAGS: u32 = (1 << 15) | (1 << 6);
        // SAFETY: MXCSR only changes float rounding/denormal behaviour.
        unsafe {
            let csr = _mm_getcsr();
            if csr & FLAGS != FLAGS {
                _mm_setcsr(csr | FLAGS);
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // FPCR.FZ = bit 24 (flushes inputs and outputs).
        const FZ: u64 = 1 << 24;
        // SAFETY: FPCR only changes float rounding/denormal behaviour.
        unsafe {
            let fpcr: u64;
            std::arch::asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack));
            if fpcr & FZ == 0 {
                std::arch::asm!("msr fpcr, {}", in(reg) fpcr | FZ, options(nomem, nostack));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn subnormals_become_zero_after_the_call() {
        std::thread::spawn(|| {
            let tiny = std::hint::black_box(f32::MIN_POSITIVE);
            let half = std::hint::black_box(0.5f32);
            assert!((tiny * half).is_subnormal(), "default FPU keeps subnormals");
            super::flush_to_zero();
            assert_eq!(std::hint::black_box(tiny) * half, 0.0);
        })
        .join()
        .unwrap();
    }
}
