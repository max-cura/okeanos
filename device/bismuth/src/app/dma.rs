use bcm2835_lpa::vcmailbox::write;
use volatile_register::{RO, RW};

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct DMA_TI(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        pub no_wide_bursts : bool @ 26,
        pub waits: u8 @ 21..26,
        pub permap: u8 @ 16..21,
        pub burst_length: u8 @ 12..16,
        pub src_ignore: bool @ 11,
        /// The DREQ selected by PER_MAP will gate the source reads.
        pub src_dreq: bool @ 10,
        /// 1=128-bit, 0=32-bit
        pub src_width: bool @ 9,
        /// 1=increment after each read; by 4 if SRC_WIDTH=0 else 32
        pub src_inc: bool @ 8,
        pub dest_ignore: bool @ 7,
        /// The DREQ selected by PERMAP will gate the destination writes.
        pub dest_dreq: bool @ 6,
        /// 1=128-bit, 0=32-bit
        pub dest_width: bool @ 5,
        pub dest_inc: bool @ 4,
        /// When set this makes the DMA wait until it receives the AXI write response for each
        /// write. This ensures that multiple writes cannot get stacked in the AXI bus pipeline.
        pub wait_resp: bool @ 3,
        /// 1=2D mode, 0=linear
        pub tdmode: bool @ 1,
        /// 1=Generate an interrupt when the transfer described by the current control block
        /// completes.
        pub inten: bool @ 0,
    }
}

#[repr(C, align(32))]
#[derive(Debug, Copy, Clone)]
pub struct DMA_CB {
    pub ti: DMA_TI,
    pub srce_ad: u32,
    pub dest_ad: u32,
    pub tfr_len: u32,

    pub stride: u32,
    pub next_cb: u32,
    pub debug: u32,
    pub _unused: u32,
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct DMA_CS(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        pub reset : bool @ 31,
        pub abort : bool @ 30,
        pub disdebug: bool @ 29,
        pub wait_for_outstanding_writes: bool @ 28,
        pub panic_priority: u8 @ 20..24,
        pub priority: u8 @ 16..20,
        pub error: bool @ 8,
        pub waiting_for_outstanding_writes: bool @ 6,
        pub dreq_stops_dma: bool @ 5,
        pub paused: bool @ 4,
        pub dreq: bool @ 3,
        pub int: bool @ 2,
        pub end: bool @ 1,
        pub active: bool @ 0,
    }
}

#[repr(C, align(0x100))]
pub struct DMAChannel {
    pub cs: RW<DMA_CS>,
    pub conblk_ad: RW<u32>,
    pub ti: RO<DMA_TI>,
    pub source_ad: RO<u32>,
    pub dest_ad: RO<u32>,
    pub txfr_len: RO<u32>,
    pub stride: RO<u32>,
    pub nextconbk: RO<u32>,
    pub debug: RW<u32>,
}

#[repr(C)]
pub struct DMA {
    pub devices: [DMAChannel; 15],
    // _pad: [u8; 0xe0],
    // int_status: RW<u32>,
    // _pad2: [u8; 0x0c],
    // enable: RW<u32>,
}
impl DMA {
    pub unsafe fn steal() -> &'static Self {
        unsafe { core::ptr::with_exposed_provenance_mut::<Self>(0x2000_7000).as_ref_unchecked() }
    }
}
