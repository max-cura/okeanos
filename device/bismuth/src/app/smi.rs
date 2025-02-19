use crate::steal_println;
use bcm2835_lpa::SYSTMR;
use quartz::arch::arm1176::__dsb;
use quartz::device::bcm2835::timing::delay_micros;
use volatile_register::RW;

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct CM_CTL(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        // 5a
        pub passwd : u8 @ 24..32,
        pub mash : u8 @ 9..11,
        pub flip : bool @ 8,
        pub busy : bool @ 7,
        pub kill : bool @ 5,
        pub enab : bool @ 4,
        pub src : u8 @ 0..4,
    }
}
proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct CM_DIV(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        // 5a
        pub passwd : u8 @ 24..32,
        pub divi : u16 @ 12..24,
        pub divf : u16 @ 0..12,
    }
}

#[repr(C)]
// base+1010b0
pub struct CM_SMI {
    pub ctl: RW<CM_CTL>,
    pub div: RW<CM_DIV>,
}
impl CM_SMI {
    pub unsafe fn steal() -> &'static Self {
        &*(0x201010b0 as *mut CM_SMI)
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct SMI_CS(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// RX fifo full
        pub rxf: bool @ 31,
        /// TX fifo empty
        pub txe: bool @ 30,
        /// RX fifo contains data
        pub rxd: bool @ 29,
        /// TX fifo can accept data
        pub txd: bool @ 28,
        /// RX fifo needs reading (>3/4 full OR in "DONE" state and FIFO not empty)
        pub rxr: bool @ 27,
        /// TX fifo needs writing (<1/4 full)
        pub txw: bool @ 26,
        /// AXI FIFO error: 1 when fifo read when empty or written when full.
        /// Clear bit by one.
        pub aferr: bool @ 25,

        /* NOTE: GAP */

        /// External DREQ received
        pub edreq: bool @ 15,
        /// Enable pixel transfer modes
        pub pxldat: bool @ 14,
        /// Set if there was an error writing to setup regs (e.g. tx was in progress).
        /// Write 1 to clear.
        pub seterr: bool @ 13,
        /// Enable pixel valve mode
        pub pvmode: bool @ 12,
        /// Interrupt on RX
        pub intr: bool @ 11,
        /// Interrupt on TX
        pub intt: bool @ 10,
        /// Interrupt on DONE condition
        pub intd: bool @ 9,
        /// Tear effect mode enabled: programmed transfers will wait for a TE trigger before waiting
        pub teen: bool @ 8,
        /// Padding settings for external transfers. For writes: the number of bytes initially
        /// written to the TX FIFO that should be ignored. For reads, the number of bytes that will
        /// be read before the data, and should be dropped.
        pub pad: u8 @ 6..8,
        // pub pad1: bool @ 7,
        // /// See [`pad1`]. Unclear what the difference between the two is.
        // pub pad0: bool @ 6,
        /// Transfer direction: 1 = write to external device, 2 = read
        pub write: bool @ 5,
        /// Write 1 to clear the FIFOs
        pub clear: bool @ 4,
        /// Write 1 to start the programmed transfer
        pub start: bool @ 3,
        /// Reads as 1 when a programmed transfer is underway.
        pub active: bool @ 2,
        /// Reads as 1 when transfer finished. For RX, not set until FIFO emptied.
        pub done: bool @ 1,
        /// Set to 1 to enable the SMI peripheral, 0 to disable.
        pub enable: bool @ 0,
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct SMI_A(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        pub device: u8 @ 8..10,
        pub address: u8 @ 0..6,
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct SMI_DC(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// DMA enable: set 1: DMA requests will be issued
        pub dmaen: bool @ 28,
        /// DMA passthrough.
        /// When set to 0, top two data pins used for SMI as usual.
        /// When set to 1, top two pins are used for external DREQs: pin 16 read request, 17 write.
        pub dmap: bool @ 24,
        /// threshold at which DMA will panic during read
        pub panicr: u8 @ 18..24,
        /// threshold at which DMA will panic during write
        pub panicw: u8 @ 12..18,
        /// (read) threshold at which DMA will generate a DREQ
        pub reqr: u8 @ 6..12,
        /// (write) threshold at which DMA will generate a DREQ
        pub reqw: u8 @ 0..6,
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct SMI_DSR(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// Read transfer width. 00=8bit, 01=16bit, 10=18bit, 11=9bit
        pub rwidth: u8 @ 30..32,
        /// Read setup time: number of core cycles between chip select/address and read strobe.
        /// Min 1, max 64,
        pub rsetup: u8 @ 24..30,
        /// 1 for System 68 mode (i.e. enable + direction pins, rather than OE+WE pin)
        pub mode68: bool @ 23,
        /// If set to 1, setup time only applies to first transfer after address change.
        pub fsetup: bool @ 22,
        /// Number of core cycles between read strobe going inactive and CS/address going inactive.
        /// Min 1, max 64.
        pub rhold: u8 @ 16..22,
        /// When set to 1, this device's rpace value will always be used for the next transaction,
        /// even if it is not to this device.
        pub rpaceall: bool @ 15,
        /// Number of core cycles spent waiting between CS deassert and start of next transfer.
        /// Min 1, max 128.
        pub rpace: u8 @ 8..15,
        /// 1=use external DMA request on SD16 to pace reads from device. Must also set DMAP in CS.
        pub rdreq: bool @ 7,
        /// Number of cycles to assert the read strobe. Min 1, max 128.
        pub rstrobe: u8 @ 0..7,
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct SMI_DSW(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// Write transfer width. 00=8bit, 01=16bit, 10=18bit, 11=9bit.
        pub wwidth: u8 @ 30..32,
        /// Number of cycles between CS assert and write strobe. Min 1, max 64.
        pub wsetup: u8 @ 24..30,
        /// Pixel format of input. 0=16bit RGB 565, 1=32bit RGBA 8888.
        pub wformat: bool @ 23,
        /// 1=swap pixel data bits. (Use with CS.PXLDAT)
        pub wswap: bool @ 22,
        /// Time between WE deassert and CS deassert. Min 1, max 64.
        pub whold: u8 @ 16..22,
        /// 1=This device's WPACE will be used for the next transfer, regardless of that transfer's
        /// device.
        pub wpaceall: bool @ 15,
        /// Cycles between CS deassert and next CS assert. Min 1, max 128.
        pub wpace: u8 @ 8..15,
        /// Use external DREQ on pin 17 to pace writes. DMAP must be set in CS.
        pub wdreq: bool @ 7,
        /// Number of cycles to assert the write strobe. Min 1, max 128.
        pub wstrobe: u8 @ 0..7,
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct SMI_DCS(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// Direction of transfer: 1 -> write, 0 -> read.
        pub write: bool @ 3,
        /// 1 when a transfer has finished. Write 1 to clear.
        pub done: bool @ 2,
        /// Write 1 to start a transfer, if one is not already underway.
        pub start: bool @ 1,
        /// Write 1 to enable SMI in direct mode.
        pub enable: bool @ 0,
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct SMI_DA(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// Indicates which of the device settings banks should be used.
        pub device: u8 @ 8..10,
        /// The value to be asserted on the address pins.
        pub address: u8 @ 0..6,
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct SMI_FD(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// High-tide mark of FIFO count during the most recent transfer.
        pub flvl: u8 @ 8..14,
        /// The current FIFO count.
        pub fcnt: u8 @ 0..6,
    }
}

#[repr(C)]
pub struct SMIDevice {
    pub dsr: RW<SMI_DSR>,
    pub dsw: RW<SMI_DSW>,
}
#[repr(C)]
// base+600000
pub struct SMI {
    /// Control and status register.
    pub cs: RW<SMI_CS>,
    /// Length/count (num external transfers) register.
    /// When (D?)CS.START is set to 1, the SMI controller will read perform `l` transfers.
    pub l: RW<u32>,
    /// Address register.
    pub a: RW<SMI_A>,
    /// Data register.
    pub d: RW<u32>,
    /// Device read/write settings.
    pub devices: [SMIDevice; 4],
    /// DMA control register.
    pub dc: RW<SMI_DC>,
    /// Direct control/status register.
    pub dcs: RW<SMI_DCS>,
    /// Direct address register.
    pub da: RW<SMI_DA>,
    /// Direct data register.
    pub dd: RW<u32>,
    /// FIFO debug register.
    pub fd: RW<SMI_FD>,
}
impl SMI {
    pub unsafe fn steal() -> &'static Self {
        &*(0x20600000 as *mut SMI)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SMIDataWidth {
    Bits8 = 0,
    Bits9 = 3,
    Bits16 = 1,
    Bits18 = 2,
}

#[derive(Debug, Copy, Clone)]
pub struct SMIConfig {
    pub width: SMIDataWidth,
    pub clock_ns: u16,
    /// 64
    pub setup_cycles: u8,
    /// 128
    pub strobe_cycles: u8,
    /// 64
    pub hold_cycles: u8,
    /// 128
    pub pace_cycles: u8,
}
pub fn smi_init(st: &SYSTMR, cm_smi: &CM_SMI, smi: &SMI, config: SMIConfig) {
    assert!(config.setup_cycles >= 1 && config.setup_cycles <= 64);
    assert!(config.strobe_cycles >= 1 && config.strobe_cycles <= 128);
    assert!(config.hold_cycles >= 1 && config.hold_cycles <= 128);
    // TODO: pace cycles?

    steal_println!("config is correct");

    unsafe {
        __dsb();
        smi.cs.write(SMI_CS(0));
        smi.l.write(0);
        smi.a.write(SMI_A(0));
        for i in 0..4 {
            smi.devices[i].dsr.write(SMI_DSR(0));
            smi.devices[i].dsw.write(SMI_DSW(0));
        }
        smi.dcs.write(SMI_DCS(0));
        smi.da.write(SMI_DA(0));
        __dsb();
    }

    steal_println!("wrote zero values");

    let divi = config.clock_ns / 2;

    unsafe {
        /// SAFETY NOTE: delay_micros() has sufficient __dsb() barriers.
        __dsb();
        cm_smi
            .ctl
            .write(CM_CTL(0).with_passwd(0x5a).with_kill(true));
        delay_micros(st, 10);
        while cm_smi.ctl.read().busy() {}
        delay_micros(st, 10);
        cm_smi
            .div
            .write(CM_DIV(0).with_passwd(0x5a).with_divi(divi));
        delay_micros(st, 10);
        // 500MHz PLLD
        cm_smi
            .ctl
            .write(CM_CTL(0).with_passwd(0x5a).with_enab(true).with_src(6));
        delay_micros(st, 10);
        while !cm_smi.ctl.read().busy() {}
        delay_micros(st, 100);

        steal_println!("set clock");

        // Clear on startup
        if smi.cs.read().seterr() {
            smi.cs.modify(|r| r.with_seterr(true));
        }
        //     smi_dsr->rsetup = smi_dsw->wsetup = setup;
        //     smi_dsr->rstrobe = smi_dsw->wstrobe = strobe;
        //     smi_dsr->rhold = smi_dsw->whold = hold;
        //     smi_dsr->rwidth = smi_dsw->wwidth = width;
        smi.devices[0].dsr.modify(|r| {
            r.with_rsetup(config.setup_cycles)
                .with_rstrobe(config.strobe_cycles)
                .with_rhold(config.hold_cycles)
                .with_rwidth(config.width as u8)
                .with_rpace(config.pace_cycles)
        });
        smi.devices[0].dsw.modify(|r| {
            r.with_wsetup(config.setup_cycles)
                .with_wstrobe(config.strobe_cycles)
                .with_whold(config.hold_cycles)
                .with_wwidth(config.width as u8)
                .with_wpace(config.pace_cycles)
        });
        //     smi_dmc->panicr = smi_dmc->panicw = 8;
        //     smi_dmc->reqr = smi_dmc->reqw = 2;
        __dsb();
    }
}
