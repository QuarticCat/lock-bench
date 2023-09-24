//! Modified from https://gitlab.com/numa-spinlock/numa-spinlock

use std::{
    hint::spin_loop,
    ptr::null_mut,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

use lock_api::{GuardNoSend, RawMutex};

struct Node {
    next: AtomicPtr<Self>,
    locked: AtomicBool,
}

#[thread_local]
static mut NODE: Node = Node {
    next: AtomicPtr::new(null_mut()),
    locked: AtomicBool::new(false),
};

pub struct RawSpinlock {
    tail: AtomicPtr<Node>,
}

unsafe impl RawMutex for RawSpinlock {
    const INIT: RawSpinlock = RawSpinlock {
        tail: AtomicPtr::new(null_mut()),
    };

    type GuardMarker = GuardNoSend;

    fn lock(&self) {
        unsafe {
            NODE.next = AtomicPtr::new(null_mut());
            NODE.locked = AtomicBool::new(false);
        }

        let node = unsafe { &mut NODE as *mut _ };
        let prev = self.tail.swap(node, Ordering::Acquire);

        if prev.is_null() {
            return;
        }

        unsafe { (*prev).next.store(node, Ordering::Relaxed) };
        while unsafe { !NODE.locked.load(Ordering::Acquire) } {
            spin_loop();
        }
    }

    fn try_lock(&self) -> bool {
        unsafe {
            NODE.next = AtomicPtr::new(null_mut());
            NODE.locked = AtomicBool::new(false);
        }

        let node = unsafe { &mut NODE as *mut _ };
        self.tail
            .compare_exchange(null_mut(), node, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    unsafe fn unlock(&self) {
        let node = &mut NODE as *mut _;
        let mut next = NODE.next.load(Ordering::Relaxed);

        if next.is_null() {
            if self
                .tail
                .compare_exchange(node, null_mut(), Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
            loop {
                next = NODE.next.load(Ordering::Relaxed);
                if !next.is_null() {
                    break;
                }
                spin_loop();
            }
        }

        unsafe { (*next).locked.store(true, Ordering::Release) };
    }
}
