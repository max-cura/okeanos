use alloc::vec;
use bcm2835_lpa::{GPIO, SYSTMR};
use bismuth::arch::arm1176::__dsb;
use bismuth::arch::arm1176::pmm::RegionKind;
use bismuth::peripherals::dma::{DMA, DMA_CB, DMA_CS, DMA_TI};
use bismuth::peripherals::smi::{smi_init, SMIConfig, SMIDataWidth, CM_SMI, SMI};
use volatile_register::RW;

const PAYLOAD_WRITE: u32 = 0x8000_0000;
const PAYLOAD_ADDRESS_MASK: u32 = 0x3fff_fffc;
const PAYLOAD_SIZE_MASK: u32 = 0x0000_0003;
const PAYLOAD_SIZE_SMALL_PAGE: u32 = 0x0000_0000;
const PAYLOAD_SIZE_LARGE_PAGE: u32 = 0x0000_0001;
const PAYLOAD_SIZE_SECTION: u32 = 0x0000_0002;
const PAYLOAD_SIZE_SUPERSECTION: u32 = 0x0000_0003;

const SMI_D_REG_PHY: *mut u32 = 0x2060_000cusize as *mut u32;
const DMA_ENABLE_REG: *mut RW<u32> = 0x2000_7ff0usize as *mut RW<u32>;

const DMA_PERMAP_SMI: u8 = 4;

fn mem_ptr_to_bus_addr(p: *mut u32) -> u32 {
    // FUCK YOU BROADCOM
    // I WASTED 5 HOURS BECAUSE YOU GAVE ME THE WRONG GODDAMN VALUE
    0x4000_0000 + (p as usize as u32)
}
fn reg_ptr_to_bus_addr(p: *mut u32) -> u32 {
    0x7e00_0000 + ((p as usize as u32) - 0x2000_0000)
}

pub fn dmr_init(smi: &SMI, cm_smi: &CM_SMI, dma: &DMA, st: &SYSTMR) {
    __dsb();
    smi_init(
        st,
        &cm_smi,
        &smi,
        SMIConfig {
            width: SMIDataWidth::Bits16, // 1 us
            clock_ns: 1000,              // 1us
            setup_cycles: 15,
            strobe_cycles: 40,
            hold_cycles: 15,
            pace_cycles: 0,
        },
    );
    __dsb();
    unsafe {
        (&*DMA_ENABLE_REG).write(1 << 5);

        smi.dc.modify(|r| r.with_dmap(true));
        // smi.devices[0].dsw.modify(|r| r.with_wdreq(true));

        // smi.devices[0].dsw.modify(|r| r.with_wdreq(true));

        smi.cs.modify(|r| r.with_pxldat(true));
        smi.cs.modify(|r| r.with_clear(true));
        smi.cs.modify(|r| r.with_clear(true));
        smi.cs.modify(|r| r.with_seterr(true));
        smi.cs.modify(|r| r.with_aferr(true));
        smi.dc
            .modify(|r| r.with_reqr(2).with_reqw(2).with_panicr(8).with_panicw(8));
        smi.dcs.modify(|r| r.with_enable(true));
    }
    __dsb();
}

pub fn dmr_issue_read_command(
    remote_address: u32,
    kind: RegionKind,
    local_ptr: *mut u32,
    smi: &SMI,
    dma: &DMA,
    gpio: &GPIO,
    device_addr: u8,
) {
    assert_eq!(
        remote_address & PAYLOAD_ADDRESS_MASK,
        remote_address,
        "remote_address doesn't fit in payload_address_mask"
    );
    let command = remote_address
        | match kind {
            RegionKind::SmallPage => PAYLOAD_SIZE_SMALL_PAGE,
            RegionKind::LargePage => PAYLOAD_SIZE_LARGE_PAGE,
            RegionKind::Section => PAYLOAD_SIZE_SECTION,
            RegionKind::Supersection => PAYLOAD_SIZE_SUPERSECTION,
        };
    assert_eq!(
        command & 0x00c000c0,
        0,
        "ERROR: while RX and TX are disconnected, they CANNOT be used in the request address"
    );

    unsafe {
        // Disable SMI so we can change parameters
        smi.cs.modify(|r| r.with_enable(false));
        // Clear FIFOs and switch to read mode
        smi.cs.modify(|r| r.with_clear(true).with_write(true));

        smi.cs.modify(|r| r.with_pxldat(true));
        smi.cs.modify(|r| r.with_enable(true));
        smi.cs.modify(|r| r.with_clear(true));
    }

    unsafe {
        smi.a.modify(|r| r.with_address(device_addr));
        smi.da.modify(|r| r.with_address(device_addr));
    }

    let dma_cbs = vec![
        DMA_CB {
            ti: DMA_TI(0)
                .with_dest_dreq(true)
                .with_permap(DMA_PERMAP_SMI)
                .with_src_inc(true),
            srce_ad: mem_ptr_to_bus_addr(core::ptr::addr_of!(command).cast_mut()),
            dest_ad: reg_ptr_to_bus_addr(SMI_D_REG_PHY),
            tfr_len: 4,
            stride: 0,
            next_cb: 0,
            debug: 0,
            _unused: 0,
        },
        DMA_CB {
            ti: DMA_TI(0)
                .with_src_dreq(true)
                .with_dest_inc(true)
                .with_permap(DMA_PERMAP_SMI)
                .with_wait_resp(true),
            srce_ad: reg_ptr_to_bus_addr(SMI_D_REG_PHY),
            dest_ad: mem_ptr_to_bus_addr(local_ptr),
            tfr_len: kind.size() as u32,
            stride: 0,
            next_cb: 0,
            debug: 0,
            _unused: 0,
        },
    ];

    __dsb();

    unsafe {
        smi.l.write(2);
        smi.dc.modify(|r| r.with_dmaen(true));
        smi.cs.modify(|r| r.with_write(true));
        smi.cs.modify(|r| r.with_enable(true));
        smi.cs.modify(|r| r.with_clear(true));
    }

    __dsb();

    // SAFETY: ensure that dma_cbs isn't dropped until after transactions have completed
    let cbs_ptr = dma_cbs.as_slice().as_ptr();

    unsafe {
        dma.devices[5].cs.write(DMA_CS(0).with_reset(true));
    }

    unsafe {
        dma.devices[5]
            .conblk_ad
            .write(mem_ptr_to_bus_addr(cbs_ptr.cast::<u32>().cast_mut()));
        dma.devices[5].cs.write(DMA_CS(0).with_end(true));
        dma.devices[5].debug.write(7);
        dma.devices[5].cs.write(DMA_CS(0).with_active(true));
    }

    __dsb();

    smi_direct_write16(&smi, 0xffff);

    __dsb();

    unsafe {
        smi.cs.modify(|r| r.with_start(true));
    }

    __dsb(); // barrier: SMI to DMA

    // Wait for DMA to complete
    while dma.devices[5].txfr_len.read() > 0 {}
    while dma.devices[5].cs.read().active() {}

    __dsb(); // barrier: DMA to SMI

    // Wait for SMI to finish
    while !smi.cs.read().done() {}

    unsafe {
        // Disable SMI so we can change parameters
        smi.cs.modify(|r| r.with_enable(false));
        // Clear FIFOs and switch to read mode
        smi.cs.modify(|r| r.with_clear(true).with_write(false));
        // Set transaction size to number of u16's that will be read
        smi.l.write({ kind.size() / 2 } as u32);

        smi.cs.modify(|r| r.with_pxldat(true));
        smi.cs.modify(|r| r.with_enable(true));
        smi.cs.modify(|r| r.with_clear(true));
    }

    __dsb();

    unsafe {
        dma.devices[5].conblk_ad.write(mem_ptr_to_bus_addr(
            cbs_ptr.offset(1).cast::<u32>().cast_mut(),
        ));
        dma.devices[5].cs.write(DMA_CS(0).with_end(true));
        dma.devices[5].debug.write(7);
    }

    __dsb();

    unsafe { gpio.gpset0().write_with_zero(|w| w.set27().set_bit()) }

    while gpio.gplev0().read().lev26().bit_is_set() {}

    __dsb();

    unsafe {
        dma.devices[5].cs.write(DMA_CS(1));
    }

    __dsb();
    __dsb();

    unsafe {
        smi.cs.modify(|r| r.with_start(true));
    }

    __dsb();

    while dma.devices[5].txfr_len.read() > 0 {}
    while dma.devices[5].cs.read().active() {}

    __dsb();

    // Okay, transfer finished
    let _ = dma_cbs;
}

pub fn dmr_issue_write_command(
    remote_address: u32,
    kind: RegionKind,
    local_ptr: *const u32,
    smi: &SMI,
    dma: &DMA,
    gpio: &GPIO,
    device_addr: u8,
) {
    assert_eq!(
        remote_address & PAYLOAD_ADDRESS_MASK,
        remote_address,
        "remote_address doesn't fit in payload_address_mask"
    );
    let command = remote_address
        | PAYLOAD_WRITE
        | match kind {
            RegionKind::SmallPage => PAYLOAD_SIZE_SMALL_PAGE,
            RegionKind::LargePage => PAYLOAD_SIZE_LARGE_PAGE,
            RegionKind::Section => PAYLOAD_SIZE_SECTION,
            RegionKind::Supersection => PAYLOAD_SIZE_SUPERSECTION,
        };
    assert_eq!(
        command & 0x00c000c0,
        0,
        "ERROR: while RX and TX are disconnected, they CANNOT be used in the request address"
    );

    unsafe {
        // Disable SMI so we can change parameters
        smi.cs.modify(|r| r.with_enable(false));
        // Clear FIFOs and switch to read mode
        smi.cs.modify(|r| r.with_clear(true).with_write(true));

        // smi.cs.modify(|r| r.with_pxldat(true));
        smi.cs.modify(|r| r.with_enable(true));
        smi.cs.modify(|r| r.with_clear(true));
    }

    unsafe {
        smi.a.modify(|r| r.with_address(device_addr));
        smi.da.modify(|r| r.with_address(device_addr));
    }

    let dma_cbs = vec![
        DMA_CB {
            ti: DMA_TI(0)
                .with_dest_dreq(true)
                .with_permap(DMA_PERMAP_SMI)
                .with_src_inc(true),
            srce_ad: mem_ptr_to_bus_addr(core::ptr::addr_of!(command).cast_mut()),
            dest_ad: reg_ptr_to_bus_addr(SMI_D_REG_PHY),
            tfr_len: 4,
            stride: 0,
            next_cb: 0,
            debug: 0,
            _unused: 0,
        },
        DMA_CB {
            ti: DMA_TI(0)
                .with_dest_dreq(true)
                .with_permap(DMA_PERMAP_SMI)
                .with_src_inc(true),
            srce_ad: mem_ptr_to_bus_addr(local_ptr.cast_mut()),
            dest_ad: reg_ptr_to_bus_addr(SMI_D_REG_PHY),
            tfr_len: kind.size() as u32,
            stride: 0,
            next_cb: 0,
            debug: 0,
            _unused: 0,
        },
    ];

    __dsb();

    unsafe {
        smi.l.write(2);
        smi.dc.modify(|r| r.with_dmaen(true));
        smi.cs.modify(|r| r.with_write(true));
        smi.cs.modify(|r| r.with_enable(true));
        smi.cs.modify(|r| r.with_clear(true));
    }

    __dsb();

    // SAFETY: ensure that dma_cbs isn't dropped until after transactions have completed
    let cbs_ptr = dma_cbs.as_slice().as_ptr();

    unsafe {
        dma.devices[5].cs.write(DMA_CS(0).with_reset(true));
    }

    unsafe {
        dma.devices[5]
            .conblk_ad
            .write(mem_ptr_to_bus_addr(cbs_ptr.cast::<u32>().cast_mut()));
        dma.devices[5].cs.write(DMA_CS(0).with_end(true));
        dma.devices[5].debug.write(7);
        dma.devices[5].cs.write(DMA_CS(0).with_active(true));
    }

    __dsb();

    smi_direct_write16(&smi, 0xffff);

    __dsb();

    unsafe {
        smi.cs.modify(|r| r.with_start(true));
    }

    __dsb(); // barrier: SMI to DMA

    // Wait for DMA to complete
    while dma.devices[5].txfr_len.read() > 0 {}
    while dma.devices[5].cs.read().active() {}

    __dsb(); // barrier: DMA to SMI

    // Wait for SMI to finish
    while !smi.cs.read().done() {}

    unsafe {
        // // Disable SMI so we can change parameters
        // smi.cs.modify(|r| r.with_enable(false));
        // // Clear FIFOs and switch to read mode
        // smi.cs.modify(|r| r.with_clear(true).with_write(false));

        // Set transaction size to number of u16's that will be read
        smi.l.write({ kind.size() / 2 } as u32);
        //
        // smi.cs.modify(|r| r.with_pxldat(true));
        // smi.cs.modify(|r| r.with_enable(true));
        // smi.cs.modify(|r| r.with_clear(true));
    }

    __dsb();

    unsafe {
        dma.devices[5].conblk_ad.write(mem_ptr_to_bus_addr(
            cbs_ptr.offset(1).cast::<u32>().cast_mut(),
        ));
        dma.devices[5].cs.write(DMA_CS(0).with_end(true));
        dma.devices[5].debug.write(7);
    }

    __dsb();

    while gpio.gplev0().read().lev26().bit_is_set() {}

    __dsb();

    unsafe {
        dma.devices[5].cs.write(DMA_CS(1));
    }

    __dsb();
    __dsb();

    unsafe {
        smi.cs.modify(|r| r.with_start(true));
    }

    __dsb();

    while dma.devices[5].txfr_len.read() > 0 {}
    while dma.devices[5].cs.read().active() {}

    __dsb();

    // Okay, transfer finished
    let _ = dma_cbs;
}

fn smi_direct_write16(smi: &SMI, val: u16) {
    unsafe {
        __dsb();
        smi.dcs.modify(|r| r.with_done(true).with_write(true));
        smi.dd.write(val as u32);
        smi.dcs.modify(|r| r.with_start(true));
        __dsb();
    }
}
