use crate::arch::arm1176::mmu::MMUEnabledFeaturesConfig;
use crate::symbols::{__symbol_bss_end__, __symbol_bss_start__, __symbol_exec_end__};
use crate::{arch, uart1_sendln_bl};
use bcm2835_lpa::Peripherals;
use core::ptr::addr_of;

struct Booted {}

/// Run boot process.
pub fn boot() {
    zero_bss();

    let peri = unsafe { Peripherals::steal() };
    // 270=115200
    crate::muart::__uart1_init(&peri.GPIO, &peri.AUX, &peri.UART1, 270);

    // 100ms to give theseus-upload enough time to switch baud rate
    crate::arch::arm1176::timing::delay_millis(&peri.SYSTMR, 100);
    uart1_sendln_bl!("[bismuth]:\tbeginning boot process");

    uart1_sendln_bl!("symbol_exec_end: {:p}", unsafe {
        addr_of!(__symbol_exec_end__)
    });

    uart1_sendln_bl!("Initializing MMU");

    let ttb = ttb_static::TTB_REGION.as_ptr().cast_mut();
    unsafe {
        arch::arm1176::mmu::__init_mmu(ttb.cast());
    }

    uart1_sendln_bl!("Finished initializing MMU");

    unsafe {
        arch::arm1176::mmu::__set_mmu_enabled_features(MMUEnabledFeaturesConfig {
            dcache: Some(false),
            icache: Some(false),
            brpdx: Some(true),
        })
    }

    uart1_sendln_bl!("MMU: +dcache +icache +brpdx");

    let mut pmm = PMM.get().lock();
    unsafe {
        (&mut pmm).initialize_once(&[(
            0 as *mut u8,
            addr_of!(crate::symbols::__symbol_exec_end__).cast_mut(),
        )])
    }

    uart1_sendln_bl!("Built PMM");
    uart1_sendln_bl!("{pmm:?}");

    uart1_sendln_bl!("[bis]: setting dcache=+1 icache=+1 brpdx=+1");

    unsafe {
        arch::arm1176::mmu::__set_mmu_enabled_features(MMUEnabledFeaturesConfig {
            dcache: Some(true),
            icache: Some(true),
            brpdx: Some(true),
        })
    }

    uart1_sendln_bl!("[bis]: boot process finished");
}

mod ttb_static {
    #[repr(C, align(0x4000))]
    pub struct TTBRegion([u8; 0x4000]);
    impl TTBRegion {
        pub fn as_ptr(&self) -> *const u8 {
            self.0.as_ptr()
        }
    }
    pub static TTB_REGION: TTBRegion = TTBRegion([0; 0x4000]);
}
// optimization: this lets us go into BSS
mod pmm_static {
    use crate::arch::arm1176::pmm::PMM;
    use crate::sync::once::OnceLockInit;
    use crate::sync::ticket::TicketLock;
    use core::mem::size_of;

    static PMM_REGION: [u8; size_of::<PMM>()] = [0; size_of::<PMM>()];
    pub static PMM: OnceLockInit<
        TicketLock<&'static mut PMM>,
        fn() -> TicketLock<&'static mut PMM>,
    > = OnceLockInit::new(|| {
        TicketLock::new(unsafe {
            crate::arch::arm1176::pmm::pmm_init_at(
                core::ptr::NonNull::new(PMM_REGION.as_ptr().cast::<PMM>().cast_mut()).unwrap(),
            )
        })
    });
}
pub use pmm_static::PMM;

fn zero_bss() {
    unsafe {
        let start = addr_of!(__symbol_bss_start__);
        let end = addr_of!(__symbol_bss_end__);
        if end < start {
            // we're fucked, I tell ya, fucked
            _halt();
        }
        let bss_len = end.byte_offset_from(start) as usize;
        core::ptr::write_bytes(start.cast_mut(), 0, bss_len);
    }
}

fn _halt() -> ! {
    loop {}
}
