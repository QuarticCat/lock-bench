//! Modified from https://probablydance.com/2019/12/30/measuring-mutexes-spinlocks-and-how-bad-the-linux-scheduler-really-is/

use std::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

use lock_api::{GuardSend, RawMutex};

pub struct RawSpinlock {
    locked: AtomicBool,
}

unsafe impl RawMutex for RawSpinlock {
    const INIT: RawSpinlock = RawSpinlock {
        locked: AtomicBool::new(false),
    };

    type GuardMarker = GuardSend;

    fn lock(&self) {
        loop {
            let was_locked = self.locked.load(Ordering::Relaxed);
            if !was_locked
                && self
                    .locked
                    .compare_exchange_weak(was_locked, true, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            {
                break;
            }
            spin_loop()
        }
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
