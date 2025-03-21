#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(sync_unsafe_cell)]
#![feature(ptr_as_ref_unchecked)]
#![feature(slice_ptr_get)]
#![feature(pointer_is_aligned_to)]
#![feature(array_try_map)]
#![feature(fn_ptr_trait)]
#![feature(array_ptr_get)]
#![no_std]

mod app;
mod exceptions;
pub mod fmt;
mod int;
mod mmu;

extern crate alloc;

use crate::fmt::Uart1WriteProxy;
use bcm2835_lpa::Peripherals;
use quartz::arch::arm1176::mmu::MMUEnabledFeaturesConfig;
use quartz::device::bcm2835::mini_uart::baud_to_clock_divider;

unsafe extern "C" {
    static __symbol_bss_start__: [u32; 0];
    static __symbol_bss_end__: [u32; 0];
    static __symbol_stack_init__: [u32; 0];

    static __symbol_args_begin__: [u8; 0x100];
}

/*
This is the entry point for the entire system.
 */
core::arch::global_asm!(r#"
    .section ".text.start"
    .globl _start
    _start:
        mrs r0, cpsr
        and r0, r0, {CLEAR_MODE_MASK}
        orr r0, r0, {SUPER_MODE}
        orr r0, r0, {CLEAR_MODE_IRQ_FIQ}
        msr cpsr, r0
        mov r0, #0
        mcr p15, 0, r0, c7, c5, 4
        mov r0, #0
        ldr r1, ={BSS_START}
        ldr r2, ={BSS_END}
        subs r2, r2, r1
        bcc 3f
    2:
        strb r0, [r1], #1
        subs r2, r2, #1
        bne 2b
    3:
        ldr sp, ={STACK_INIT}
        mov fp, #0
        bl {KERNEL_START}
        bl {KERNEL_RESTART}
    "#,
    CLEAR_MODE_MASK = const !0b11111u32,
    SUPER_MODE = const 0b10011u32,
    CLEAR_MODE_IRQ_FIQ = const (1u32 << 7) | (1u32 << 6),
    BSS_START = sym __symbol_bss_start__,
    BSS_END = sym __symbol_bss_end__,
    STACK_INIT = sym __symbol_stack_init__,
    KERNEL_START = sym __kernel_start,
    KERNEL_RESTART = sym __kernel_restart,
);

#[macro_export]
macro_rules! steal_println {
    ($($e:tt)*) => {
        let peri = unsafe { ::bcm2835_lpa::Peripherals::steal() };
        $crate::uart1_println!(&peri.UART1, $($e)*)
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn __aeabi_unwind_cpp_pr0() {}

#[global_allocator]
static HEAP: embedded_alloc::TlsfHeap = embedded_alloc::TlsfHeap::empty();

const DEFAULT_CLOCK_DIVIDER: u16 = baud_to_clock_divider(115200);

fn arg_count() -> usize {
    let arg_byte_slice = unsafe { &__symbol_args_begin__ };
    let slice_count = u32::from_le_bytes(arg_byte_slice[0..4].try_into().unwrap()) as usize;
    slice_count
}

fn get_nth_arg(idx: usize) -> Option<&'static str> {
    let arg_byte_slice = unsafe { &__symbol_args_begin__ };
    let slice_count = arg_count();
    if idx >= slice_count {
        return None;
    }
    let mut j = 4usize;
    for _ in 0..idx {
        let slice_len = u32::from_le_bytes(arg_byte_slice[j..j + 4].try_into().unwrap()) as usize;
        j += 4 + ((slice_len + 3) & !3);
    }
    let len = u32::from_le_bytes(arg_byte_slice[j..j + 4].try_into().unwrap()) as usize;
    let slice = &arg_byte_slice[j + 4..j + 4 + len];
    Some(core::str::from_utf8(slice).expect("Invalid UTF-8 in argument"))
}

#[unsafe(no_mangle)]
pub extern "C" fn __kernel_start() -> ! {
    let peripherals = unsafe { Peripherals::steal() };
    quartz::device::bcm2835::mini_uart::muart1_init(
        &peripherals.GPIO,
        &peripherals.AUX,
        &peripherals.UART1,
        DEFAULT_CLOCK_DIVIDER,
    );
    quartz::device::bcm2835::timing::delay_millis(&peripherals.SYSTMR, 100);

    steal_println!("\nSuccessfully initialized UART1.");

    // unsafe {
    //     quartz::arch::arm1176::mmu::__init_mmu(mmu_support::get_translation_table().cast());
    //     quartz::arch::arm1176::mmu::__set_mmu_enabled_features(MMUEnabledFeaturesConfig {
    //         dcache: Some(false),
    //         icache: Some(false),
    //         brpdx: Some(false),
    //     })
    // }

    unsafe { HEAP.init(0x1000_0000, 0x1000_0000) };

    if arg_count() > 0 {
        let arg0 = get_nth_arg(0).expect("Failed to get nth argument");
        match arg0 {
            "cpuid" => app::cpuid::dump_cpu_info(),
            "debug" => app::debug::interleave_checker(),
            _ => {
                steal_println!("unknown command {arg0}");
            }
        }
    }

    quartz::device::bcm2835::mini_uart::mini_uart1_flush_tx(&peripherals.UART1);

    __kernel_restart();
}

mod mmu_support {
    use core::cell::SyncUnsafeCell;

    const TRANSLATION_TABLE_SIZE: usize = 0x4000;
    #[repr(C, align(0x4000))]
    struct RawTranslationTable([u8; TRANSLATION_TABLE_SIZE]);
    impl RawTranslationTable {
        pub fn as_mut_ptr(&self) -> *mut [u8; TRANSLATION_TABLE_SIZE] {
            (&raw const self.0).cast_mut()
        }
    }

    static TRANSLATION_TABLE: SyncUnsafeCell<RawTranslationTable> =
        SyncUnsafeCell::new(RawTranslationTable([0; TRANSLATION_TABLE_SIZE]));

    pub fn get_translation_table() -> *mut [u8; TRANSLATION_TABLE_SIZE] {
        unsafe { TRANSLATION_TABLE.get().as_ref_unchecked() }.as_mut_ptr()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __kernel_restart() -> ! {
    let peripherals = unsafe { Peripherals::steal() };
    steal_println!("Rebooting.");
    quartz::device::bcm2835::watchdog::restart(&peripherals.PM)
}

#[panic_handler]
fn __kernel_panic_handler(info: &core::panic::PanicInfo) -> ! {
    let peripherals = unsafe { Peripherals::steal() };
    quartz::device::bcm2835::mini_uart::muart1_init(
        &peripherals.GPIO,
        &peripherals.AUX,
        &peripherals.UART1,
        DEFAULT_CLOCK_DIVIDER,
    );
    if let Some(loc) = info.location() {
        uart1_println!(
            &peripherals.UART1,
            "Panic occurred at file '{}' line {}:\n",
            loc.file(),
            loc.line()
        );
    } else {
        uart1_println!(&peripherals.UART1, "Panic occurred at unknown location.\n");
    }
    let msg = info.message();
    let mut proxy = Uart1WriteProxy::new(&peripherals.UART1);
    let _ = core::fmt::write(&mut proxy, format_args!("{}\n", msg));

    __kernel_restart();
}

/// We don't have built-in support from `critical_section` crate, so we need to implement our own
/// here.
mod critical_section_support {
    use crate::int::InterruptMode;
    use critical_section::RawRestoreState;

    struct BisCriticalSection;
    critical_section::set_impl!(BisCriticalSection);
    impl TryFrom<RawRestoreState> for InterruptMode {
        type Error = RawRestoreState;

        fn try_from(value: RawRestoreState) -> Result<Self, Self::Error> {
            match value {
                0 => Ok(InterruptMode::Neither),
                1 => Ok(InterruptMode::FiqOnly),
                2 => Ok(InterruptMode::IrqOnly),
                3 => Ok(InterruptMode::Both),
                _ => Err(value),
            }
        }
    }
    impl From<InterruptMode> for RawRestoreState {
        fn from(value: InterruptMode) -> Self {
            value as Self
        }
    }
    unsafe impl critical_section::Impl for BisCriticalSection {
        unsafe fn acquire() -> RawRestoreState {
            let previous_mode = super::int::set_enabled_interrupts(InterruptMode::Neither);
            previous_mode.into()
        }

        unsafe fn release(restore_state: RawRestoreState) {
            super::int::set_enabled_interrupts(
                restore_state
                    .try_into()
                    .expect("invalid critical section restoration token"),
            );
        }
    }
}
