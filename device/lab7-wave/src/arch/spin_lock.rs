use core::arch::asm;
use core::sync::atomic::{AtomicU32, Ordering};
use lock_api::{GuardSend, RawMutex, RawMutexFair};
use crate::arch::barrier::data_memory_barrier;

// Implementation details:
#[repr(C)]
#[derive(Copy, Clone)]
union RSLImpl {
    raw: u32,
    cooked: RSLCooked,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct RSLCooked {
    ticket: u16,
    owner: u16,
}

// Public-facing API:

/// Raw ticketing spinlock.
/// Reference: ARM arch_spinlock_t in torvalds/linux.
#[repr(transparent)]
#[derive(Debug)]
pub struct RawSpinLock {
    inner: AtomicU32,
}

impl RawSpinLock {
    pub const fn new() -> Self {
        Self { inner: AtomicU32::new(0) }
    }
}

impl RawSpinLock {
    pub fn is_contended(&self) -> bool {
        let state = self.inner.load(Ordering::SeqCst);
        unsafe {
            let RSLImpl { cooked: RSLCooked { ticket, owner } } = RSLImpl { raw: state };
            // an uncontended lock is either:
            //  unlocked (ticket == owner)
            //  or locked with no waiting threads: ticket == owner + 1
            // thus the test for contention is apparent:
            (ticket - owner) > 1
        }
    }
}

unsafe impl RawMutex for RawSpinLock {
    const INIT: Self = Self::new();
    type GuardMarker = GuardSend;

    fn lock(&self) {
        return;

        let wait_state : u32;

        unsafe {
            asm!(
                "2:",
                "ldrex {t0}, [{ptr}]",
                "add {t1}, {t0}, #(1 << 16)",
                "strex {t2}, {t1}, [{ptr}]",
                "teq {t2}, #0",
                "bne 2b",
                t0 = out(reg) wait_state,
                t1 = out(reg) _,
                t2 = out(reg) _,
                ptr = in(reg) self.inner.as_ptr(),
            );
        }

        let wait_state = RSLImpl { raw: wait_state };

        let RSLCooked { ticket, owner } = unsafe { wait_state.cooked };
        if core::intrinsics::unlikely(ticket != owner) {
            loop {
                unsafe { asm!("wfe") };

                let raw_update = self.inner.load(Ordering::SeqCst);
                let new_owner = unsafe { RSLImpl { raw: raw_update }.cooked.owner };

                if ticket == new_owner {
                    break
                }
            }
        }

        // TODO: DMB
        data_memory_barrier();
    }

    // true if lock acquired
    fn try_lock(&self) -> bool {
        let contended : u32;
        const IS_CONTENDED : u32 = 0;

        unsafe {
            asm!(
                "2:",
                "ldrex {t0}, [{ptr}]",
                "mov {t2}, #0",
                "subs {t1}, {t0}, {t0}, ror #16",
                "addeq {t0}, {t0}, #(1 << 16)",
                "strexeq {t2}, {t0}, [{ptr}]",
                "teq {t2}, #0",
                "bne 2b",
                t0 = out(reg) _,
                t1 = out(reg) contended,
                t2 = out(reg) _,
                ptr = in(reg) self.inner.as_ptr(),
            );
        }

        if contended == IS_CONTENDED {
            // TODO: DMB
            false
        } else {
            true
        }
    }

    #[inline(never)]
    unsafe fn unlock(&self) {
        // TODO: DMB
        // data_memory_barrier();

        let ptr = self.inner.as_ptr();
        // let lo_ptr = ptr as *mut u16;
        // let lo_ptr = &unsafe { { &*(ptr as *mut RSLImpl) }.cooked }.owner as *const u16 as *mut u16;

        // SAFETY(write to AtomicU32::as_ptr()): still atomic!

        // crate::arch::barrier::data_synchronization_barrier();
        // let peripherals = unsafe { bcm2835_lpa::Peripherals::steal() };
        // peripherals.GPIO.gpfsel2().modify(|_, w| w.fsel27().output());
        // unsafe { peripherals.GPIO.gpset0().write_with_zero(|w| w.set27().set_bit()) };
        // crate::arch::barrier::data_synchronization_barrier();

        // let _ = core::intrinsics::atomic_xadd_seqcst(ptr, 1);
        asm!(
            "2:",
        // LDREX needs MMU...
            "ldr {t0}, [{ptr}]",
            // "add {t0}, {t0}, #1",
            // "strexh {t1}, {t0}, [{ptr}]",
            // "cmp {t1}, #0",
            // "bne 2b",
            t0 = out(reg) _,
            // t1 = out(reg) _,
            ptr = in(reg) ptr,
        );

        crate::arch::barrier::data_synchronization_barrier();
        let peripherals = unsafe { bcm2835_lpa::Peripherals::steal() };
        peripherals.GPIO.gpfsel2().modify(|_, w| w.fsel27().output());
        unsafe { peripherals.GPIO.gpset0().write_with_zero(|w| w.set27().set_bit()) };
        crate::arch::barrier::data_synchronization_barrier();

        // asm!("sev");
        // unsafe {
        //     let x = core::ptr::read(lo_ptr);
        //     core::ptr::write(lo_ptr, x + 1);
        // };

        asm!("sev");
    }

    fn is_locked(&self) -> bool {
        let state = self.inner.load(Ordering::SeqCst);
        unsafe {
            let RSLImpl { cooked: RSLCooked { ticket, owner } } = RSLImpl { raw: state };
            ticket != owner
        }
    }
}

unsafe impl RawMutexFair for RawSpinLock {
    #[inline]
    unsafe fn unlock_fair(&self) {
        self.unlock()
    }
}