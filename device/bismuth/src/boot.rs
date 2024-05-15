use core::alloc::Layout;
use core::ptr::addr_of;
use bcm2835_lpa::Peripherals;
use crate::arch::arm1176::mmu::MMUEnabledFeaturesConfig;
use crate::arch::arm1176::pmm::PMM;
use crate::kalloc::bump::BumpAllocator;
use crate::symbols::{__symbol_bss_end__, __symbol_bss_start__, __symbol_exec_end__};
use crate::sync::once::OnceLock;
use crate::sync::ticket::TicketLock;
use crate::uart1_sendln_bl;

struct Booted {

}

/// Run boot process.
pub fn boot() {
    zero_bss();

    let peri = unsafe { Peripherals::steal() };
    // 270=115200
    crate::muart::__uart1_init(&peri.GPIO, &peri.AUX, &peri.UART1, 270);

    // 100ms to give theseus-upload enough time to switch baud rate
    crate::arch::arm1176::timing::delay_millis(&peri.SYSTMR, 100);
    uart1_sendln_bl!("[bismuth]:\tbeginning boot process");

    uart1_sendln_bl!("symbol_exec_end: {:p}", unsafe { addr_of!(__symbol_exec_end__) });

    let mut bump = unsafe { BumpAllocator::new(
        addr_of!(__symbol_exec_end__).cast_mut(),
        0x2000_0000usize as *mut u8
    ) };

    let ttb = match bump.allocate_pages(unsafe { Layout::from_size_align_unchecked(0x4000, 0x4000) }) {
        None => _halt(),
        Some(p) => p,
    };

    uart1_sendln_bl!("Initializing MMU");

    unsafe {
        crate::arch::arm1176::mmu::__init_mmu(ttb.as_mut_ptr().cast());
    }

    uart1_sendln_bl!("Finished initializing MMU");

    let pmm_space = bump.allocate_pages(Layout::new::<PMM>()).unwrap();
    let pmm = unsafe { crate::arch::arm1176::pmm::pmm_init_at(pmm_space.cast()) };
    let bumped = bump.consume();
    unsafe { pmm.initialize_once(&[(0 as *mut u8, bumped.0)]) }
    PMM.get_or_init(|| TicketLock::new(pmm));

    uart1_sendln_bl!("Built PMM");

    unsafe {
        crate::arch::arm1176::mmu::__set_mmu_enabled_features(MMUEnabledFeaturesConfig {
            dcache: Some(true),
            icache: Some(true),
            brpdx: Some(true),
        })
    }

    uart1_sendln_bl!("Features: +dcache +icache +brpdx");
}

static PMM : OnceLock<TicketLock<&'static mut PMM>> = OnceLock::new();

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

fn _halt() -> ! { loop {} }
