use core::panic::PanicInfo;
use core::ptr;
use core::sync::atomic::Ordering;
use portable_atomic::AtomicPtr;

#[panic_handler]
fn panic(info: &::core::panic::PanicInfo) -> ! {
    let ptr = PANIC_AGENT.load(Ordering::SeqCst);
    if ptr.is_null() {
        // Fallback behaviour: infinite loop
        loop {}
    } else {
        (unsafe { *ptr })(info,)
    }
}

pub type PanicFn = fn(&PanicInfo) -> !;

static PANIC_AGENT: AtomicPtr<PanicFn> = AtomicPtr::new(ptr::null_mut());

pub fn set_panic_behaviour(
    panic_fn: *const PanicFn
) {
    // SAFETY: even though we cast to *mut fn(&PanicInfo) -> !, we never write to it, so OK I think?
    PANIC_AGENT.store(panic_fn as *mut _, Ordering::SeqCst);
}