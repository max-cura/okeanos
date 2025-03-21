use crate::arch::csr::define_csr;

pub mod csr;
pub mod exception;
pub mod mem;
pub mod plic;
pub mod time;

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MxstatusPrivilegeMode {
    UMode = 0,
    SMode = 1,
    MMode = 3,
}
impl From<u8> for MxstatusPrivilegeMode {
    fn from(x: u8) -> Self {
        match x {
            0 => MxstatusPrivilegeMode::UMode,
            1 => MxstatusPrivilegeMode::SMode,
            3 => MxstatusPrivilegeMode::MMode,
            _ => unreachable!(),
        }
    }
}
impl From<MxstatusPrivilegeMode> for u8 {
    fn from(x: MxstatusPrivilegeMode) -> Self {
        x as u8
    }
}
proc_bitfield::bitfield! {
    pub struct Mxstatus(pub usize): Debug, IntoStorage, FromStorage, FromStorage {
        /// Privilege mode. See [`MxstatusPrivilegeMode`]
        pub pm : u8 [MxstatusPrivilegeMode] @ 30..=31,
        /// Enable extended instruction sets. (1 = C906 extended instructions are enabled)
        pub theadisaee : bool @ 22,
        /// Extend MMU address attribute. (1 = address attribute is extended in the PTE of the MMU;
        /// address attributes of pages can be configured)
        pub maee : bool @ 21,
        /// Disable hardware writeback. (1 = hardware writeback is not performed if the TBL is
        /// missing)
        pub mhrd : bool @ 18,
        /// CLINT timer/software interrupt supervisor extension enable. (1 = S-mode software
        /// interrupts and timer interrupts initiated by CLIENT can be responded to)
        pub clintee : bool @ 17,
        /// U-mode extended cache instructions. (1 = extended DCACHE.CIVA, DCACHE.CVA, ICACHE.IVA
        /// can be executed in U-mode)
        pub ucme : bool @ 16,
        /// Misaligned access enable. (1 = misaligned accesses are supported and processed by
        /// hardware)
        pub mm : bool @ 15,
        /// *Read-only.* The minimum PMP granularity supported by C906 is 4KB.
        pub pmp4k : bool @ 14,
        /// M-mode performance monitoring count enable. (1 = performance counters not allowed to
        /// count in M-mode)
        pub pmdm : bool @ 13,
        /// S-mode performance monitoring count enable. (1 = performance counters not allowed to
        /// count in S-mode)
        pub pmds : bool @ 11,
        /// U-mode performance monitoring count enable. (1 = performance counters not allowed to
        /// count in U-mode)
        pub pmdu : bool @ 10
    }
}
define_csr!(0x7c0 => mxstatus : crate::arch::Mxstatus, [unsafe]);

proc_bitfield::bitfield! {
    /// M-mode hardware configuration register. Configures CPU functionality.
    pub struct Mhcr(pub usize): Debug, IntoStorage, FromStorage, DerefStorage {
        /// *Read only*.
        /// Write burst transmission enable (1 = write burst transmission supported)
        pub wbr : bool [ro] @ 8,
        /// Branch target prediction enable (1 = branch target prediction enabled)
        pub btb : bool @ 6,
        /// Branch predictor enable (1 = branch prediction enabled)
        pub bpe : bool @ 5,
        /// Address return stack (1 = return stack enabled)
        pub rs : bool @ 4,
        /// Data cache writeback (1 = writeback, 0 = write-through)
        ///
        /// C906 only supports writeback mode.
        pub wb : bool [ro] @ 3,
        /// Data cache write allocate (1 = write allocate mode)
        pub wa : bool @ 2,
        /// D-cache enable (1 = active)
        pub de : bool @ 1,
        /// I-cache enable (1 = active)
        pub ie : bool @ 0,
    }
}
define_csr!(0x7c1 => mhcr : crate::arch::Mhcr, [unsafe]);

proc_bitfield::bitfield! {
    /// M-mode hardware operation register. Provides operations on caches and branch predictors.
    pub struct Mcor(pub usize): Debug, IntoStorage, FromStorage, DerefStorage {
        /// Branch target buffer (BTB) invalidate. (1 = invalidate BTBs)
        ///
        /// On read, 1 indicates that BTB invalidation is in progress, and 0 indicates that the
        /// operation has finished.
        pub btb_inv : bool @ 17,
        /// Branch history table (BHT) invalidate. (1 = invalidate BHTs)
        ///
        /// On read, 1 indicates that BTH invalidation is in progress, and 0 indicates that the
        /// operation has finished.
        pub bht_inv : bool @ 16,
        /// Dirty entry clear; this _flushes_ dirty entries in cache and writes them out of the
        /// chip. (1 = Dirty cache entries are written out of the chip)
        ///
        /// On read, 1 indicates that cache entry write-out is in progress, and 0 indicates that the
        /// operation has finished.
        pub clr : bool @ 5,
        /// Cache invalidate. (1 = invalidate caches).
        pub inv : bool @ 4,
        /// Controls behaviour of cache operations; select D-cache. Both D-cache and I-cache can be
        /// selected simultaneously.
        pub cache_sel_d : bool @ 1,
        /// Controls behaviour of cache operations; select I-cache. Both D-cache and I-cache can be
        /// selected simultaneously.
        pub cache_sel_i : bool @ 0,
    }
}
define_csr!(0x7c2 => mcor : crate::arch::Mcor);

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MhintDDis {
    /// Prefetch 2 cache lines
    L2 = 0,
    /// Prefetch 4 cache lines
    L4 = 1,
    /// Prefetch 8 cache lines (default)
    L8 = 2,
    /// Prefetch 16 cache lines
    L16 = 3,
}
impl From<u8> for MhintDDis {
    fn from(x: u8) -> Self {
        match x {
            0 => MhintDDis::L2,
            1 => MhintDDis::L4,
            2 => MhintDDis::L8,
            3 => MhintDDis::L16,
            _ => unreachable!(),
        }
    }
}
impl From<MhintDDis> for u8 {
    fn from(x: MhintDDis) -> Self {
        x as u8
    }
}
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MhintAmr {
    /// "\[W\]rite allocate policy is subject to the page attribute WA of the access address."
    Page = 0,
    /// "\[I\]f 3 consecutive cache lines are stored, storage operations of subsequent continuous
    /// addresses are no longer written to L1 Cache."
    L3 = 1,
    /// "\[I\]f 64 consecutive cache lines are stored, storage operations of subsequent continuous
    /// addresses are no longer written to L1 Cache."
    L64 = 2,
    /// "\[I\]f 128 consecutive cache lines are stored, storage operations of subsequent continuous
    /// addresses are no longer written to L1 Cache."
    L128 = 3,
}
impl From<u8> for MhintAmr {
    fn from(x: u8) -> Self {
        match x {
            0 => MhintAmr::Page,
            1 => MhintAmr::L3,
            2 => MhintAmr::L64,
            3 => MhintAmr::L128,
            _ => unreachable!(),
        }
    }
}
impl From<MhintAmr> for u8 {
    fn from(x: MhintAmr) -> Self {
        x as u8
    }
}
proc_bitfield::bitfield! {
    /// M-mode cache function control register.
    pub struct Mhint(pub usize):  Debug, IntoStorage, FromStorage, DerefStorage {
        /// Number of prefetched cache lines in D-Cache. See [`MhintDds`]
        pub d_dis : u8 [MhintDDis] @ 13..=14,
        /// I-cache way prediction enable. (1 = enabled)
        pub iwpe : bool @ 10,
        /// I-cache prefetch enable. (1 = enabled)
        pub ipld : bool @ 8,
        /// Write allocate policy automatic adjustment enable. See [`MhintAmr`]
        pub amr : u8 [MhintAmr] @ 3..=4,
        /// D-cache prefetch enable. (1 = enabled)
        pub dpld : bool @ 2,
    }
}
define_csr!(0x7c5 => mhint : crate::arch::Mhint);

proc_bitfield::bitfield! {
    /// M-mode reset vector base address register.
    pub struct Mrvbr(pub usize): Debug, IntoStorage, FromStorage, DerefStorage {
        vector_base: u64 @ 0..=39,
    }
}
define_csr!(0x7c7 => mrvbr: crate::arch::Mrvbr);

proc_bitfield::bitfield! {
    /// M-mode device address upper bits register. Specifies the 13 most significant bits of the
    /// register base addresses of PLIC and other in-core modules mounted to the APB. Read-only.
    pub struct Mapbaddr(pub usize): Debug, IntoStorage, FromStorage, DerefStorage {
        pub apbaddr_high13: u16 @ 27..=39,
    }
}
define_csr!(0xfc1 => mapbaddr : crate::arch::Mapbaddr);

proc_bitfield::bitfield! {
    pub struct Mstatus(pub usize): Debug, IntoStorage, FromStorage, DerefStorage {
        /// Dirty state sum: FPU, VP, XU.
        pub sd : bool @ 63,
        /// S-mode register bit width. Fixed to 2, meaning 64 bits.
        pub sxl : u8 @ 34..=35,
        /// U-mode register bit width. Fixed to 2, meaning 64 bits.
        pub uxl : u8 @ 32..=33,
        /// Vector status. Indicates whether to store vector registers during context switching.
        /// (00 = VU is Off and accesses will ILLEGAL, 01 = Initial, 10 = Clean, 11 = Dirty)
        /// **Fixed to 00.**
        pub vs : u8 @ 23..=24,
        /// Trap SRET. (0 = SRET can be executed in S-mode)
        pub tsr : bool @ 22,
        /// Timeout wait. (0 = WFI is allowed in S-mode)
        pub tw : bool @ 21,
        /// Trap virtual memory. (1 = illegal instruction exception occurs for reads and writes to
        /// the STAP control register and for the execution of the SFENCE instruction in S-mode,
        /// 0 = reads and writes to the STAP control register and the execution of the SFENCE
        /// instruction are allowed in S-mode)
        pub tvm : bool @ 20,
        /// Allow initiation of load requests to access memory spaces marked as executable.
        /// (1 = load requests can be initiated to access virtual memory spaces marked as executable
        /// or readable, 0 = load requests can be initiated only to access virtual memory spaces
        /// marked as readable)
        pub mxr : bool @ 19,
        /// Allow access to U-mode virtual memory address spaces in S-mode. (1 = allowed)
        /// This covers load, store, and instruction fetch requests.
        pub sum : bool @ 18,
        /// Modify privilege mode. (1 = load and store requests are executed based on MPP, 0 = load
        /// and store requests use current CPU privilege mode)
        pub mprv : bool @ 17,
        /// Extension status. Fixed to 0.
        pub xs : u8 @ 15..=16,
        /// Floating point status bit. Whether to store floating point registers during context
        /// switching. (00 = FPU is in Off state and access to registers will ILLEGAL, 01=FPU in
        /// Initial state, 10 = FPU is in Clean state, 11 = FPU is in Dirty state, which means that
        /// floating point and control registers have been modified).
        pub fs : u8 @ 13..=14,
        /// Machine preserve privilege. Privilege of the CPU before entering an exception service
        /// routine in M-mode. (00 = U-mode, 01 = S-mode, 11 = M-mode)
        pub mpp : u8 @ 11..=12,
        /// Supervisor preserve privilege. Privilege of the CPU before entering an exception service
        /// routine in S-mode. (00 = U-mode, 01 = S-mode)
        pub spp : bool @ 8,
        /// *Internal.* M-mode preserve interrupt enable. Used by the CPU to store the original
        /// value of [`mie`] while the CPU is in an M-mode exception.
        pub mpie : bool @ 7,
        /// *Internal.* S-mode preserve interrupt enable. Used by the CPU to store the original
        /// value of [`sie`] while the CPU is in an S-mode exception.
        pub spie : bool @ 5,
        /// Global M-mode interrupt enable. (1 = enabled)
        ///
        /// When the CPU enters an M-mode interrupt, this bit will be reset to zero until the CPU
        /// exits the ISR.
        pub mie : bool @ 3,
        /// Global S-mode interrupt enable. (1 = enabled)
        ///
        /// When the CPU enters an S-mode interrupt, this bit will be reset to zero until the CPU
        /// exits the ISR.
        pub sie : bool @ 1,
    }
}
define_csr!(mstatus :  crate::arch::Mstatus);

define_csr!(misa); // read-only
define_csr!(medeleg); // allow downgrade to S-mode to handle S-mode and U-mode exceptions
define_csr!(mideleg); // allow downgrade to S-mode to handle S-mode interrupts

proc_bitfield::bitfield! {
    /// M-mode interrupt enable register
    pub struct Mie(pub usize): Debug, IntoStorage, FromStorage, DerefStorage {
        /// HPM M-mode event counter overflow interrupt enable. (1 = enabled)
        pub moie : bool @ 17,
        /// M-mode external interrupt enable. (1 = enabled)
        pub meie : bool @ 11,
        /// S-mode external interrupt enable. (1 = enabled)
        pub seie : bool @ 9,
        /// M-mode timer interrupt enable. (1 = enabled)
        pub mtie : bool @ 7,
        /// S-mode timer interrupt enable. (1 = enabled)
        pub stie : bool @ 5,
        /// M-mode software interrupt enable. (1 = enabled)
        pub msie : bool @ 3,
        /// S-mode software interrupt enable. (1 = enabled)
        pub ssie : bool @ 1,
    }
}
define_csr!(mie : crate::arch::Mie);

proc_bitfield::bitfield! {
    pub struct Mtvec(pub usize): Debug, IntoStorage, FromStorage, DerefStorage {
        /// Vector base address. Upper 37 bits of the entry address of the exception service
        /// routine. The lowest two bits are automatically set to 00.
        ///
        /// Note: C906 manual gives bits `2..=63` but says 37 bits, so I force it here.
        pub base : u64 @ 2..=39,
        /// Vector entry mode. (00 = entry for both exceptions and interrupts, 01 = base address is
        /// used for exceptions, base + 4 * exception_code is used for interrupts)
        pub mode : u8 @ 0..=1,
    }
}
define_csr!(mtvec : crate::arch::Mtvec);

define_csr!(mscratch : usize);
define_csr!(mepc : usize);

proc_bitfield::bitfield! {
    pub struct Mcause(pub usize): Debug, IntoStorage, FromStorage, DerefStorage {
        pub interrupt : bool @ 63,
        exception_code: u8 @ 0..=4,
    }
}
define_csr!(mcause : crate::arch::Mcause);

define_csr!(mtval : usize);

proc_bitfield::bitfield! {
    /// M-mode interrupt pending register. Bits will be set when CPU cannot immediately respond to
    /// an interrupt.
    pub struct Mip(pub usize): Debug, IntoStorage, FromStorage, DerefStorage {
        /// M-mode overflow interrupt pending. (1 = pending)
        pub moip : bool @ 17,
        /// ?
        pub mcip : bool @ 16,
        /// M-mode external interrupt pending. (1 = pending)
        pub meip : bool @ 11,
        /// S-mode external interrupt pending. (1 = pending)
        pub seip : bool @ 9,
        /// M-mode timer interrupt pending. (1 = pending)
        pub mtip : bool @ 7,
        /// S-mode timer interrupt pending. (1 = pending)
        pub stip : bool @ 5,
        /// M-mode software interrupt pending. (1 = pending)
        pub msip : bool @ 3,
        /// S-mode software interrupt pending. (1 = pending)
        pub ssip : bool @ 1,
    }
}
define_csr!(mip : crate::arch::Mip);
