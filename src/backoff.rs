//! Modified from https://github.com/fereidani/kanal

#![allow(clippy::reversed_empty_ranges)]

use std::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
    thread::{sleep, yield_now},
    time::Duration,
};

use lock_api::{GuardSend, RawMutex};

pub struct RawSpinlock {
    locked: AtomicBool,
}

impl RawSpinlock {
    #[inline(never)]
    fn lock_no_inline(&self) {
        spin_cond(|| self.try_lock());
    }
}

unsafe impl RawMutex for RawSpinlock {
    const INIT: RawSpinlock = RawSpinlock {
        locked: AtomicBool::new(false),
    };

    type GuardMarker = GuardSend;

    fn lock(&self) {
        if self.try_lock() {
            return;
        }
        self.lock_no_inline();
    }

    fn try_lock(&self) -> bool {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    unsafe fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

fn random_u7() -> u8 {
    static SEED: AtomicU8 = AtomicU8::new(13);
    const MULTIPLIER: u8 = 113;
    let seed = SEED.fetch_add(1, Ordering::Relaxed);
    seed.wrapping_mul(MULTIPLIER) & 0x7F
}

pub fn spin_cond(cond: impl Fn() -> bool) {
    const NO_YIELD: usize = 1;
    const SPIN_YIELD: usize = 1;
    const OS_YIELD: usize = 0;
    const ZERO_SLEEP: usize = 2;
    const SPINS: u32 = 8;
    let mut spins: u32 = SPINS;

    // Short spinning phase
    for _ in 0..NO_YIELD {
        for _ in 0..SPINS / 2 {
            if cond() {
                return;
            }
            spin_loop();
        }
    }

    // Longer spinning and yielding phase
    loop {
        for _ in 0..SPIN_YIELD {
            for _ in 0..(random_u7() as usize).wrapping_add(1 << 6) {
                spin_loop();
            }
            for _ in 0..spins {
                if cond() {
                    return;
                }
            }
        }

        // Longer spinning and yielding phase with OS yield
        for _ in 0..OS_YIELD {
            yield_now();
            for _ in 0..spins {
                if cond() {
                    return;
                }
            }
        }

        // Phase with zero-length sleeping and yielding
        for _ in 0..ZERO_SLEEP {
            sleep(Duration::from_nanos(0));
            for _ in 0..spins {
                if cond() {
                    return;
                }
            }
        }

        // Geometric backoff
        if spins < (1 << 30) {
            spins <<= 1;
        }
        // Backoff about 1ms
        sleep(Duration::from_nanos(1 << 20));
    }
}
