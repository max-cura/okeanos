// This is a type alias for the enabled `restore-state-*` feature.
// For example, it is `bool` if you enable `restore-state-bool`.
use critical_section::RawRestoreState;
use lock_api::RawMutex;
use crate::arch::spin_lock::RawSpinLock;

struct MyCriticalSection;
critical_section::set_impl!(MyCriticalSection);

const TOKEN_ACQUIRE_OK : RawRestoreState = true;
#[allow(unused)]
const TOKEN_ACQUIRE_ERR : RawRestoreState = false;

static _CRSX_SPIN_LOCK : RawSpinLock = RawSpinLock::INIT;

unsafe impl critical_section::Impl for MyCriticalSection {
    unsafe fn acquire() -> RawRestoreState {
        _CRSX_SPIN_LOCK.lock();
        crate::arch::cpsr::unsafe_try_disable_irqs().is_ok()
    }

    unsafe fn release(token: RawRestoreState) {
        if token == TOKEN_ACQUIRE_OK {
            crate::arch::cpsr::unsafe_try_enable_irs().unwrap();
        }
        _CRSX_SPIN_LOCK.unlock();
    }
}

// #[no_mangle]
// pub unsafe fn _atomic_marker(x: &core::sync::atomic::AtomicU32) {
//     let _ = core::intrinsics::atomic_xadd_seqcst(core::ptr::null_mut(), 1);
//     core::arch::asm!("sev");
//     x.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
//     core::arch::asm!("sev");
//     x.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
//     core::arch::asm!("sev");
//     x.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
//     core::arch::asm!("sev");
// }
