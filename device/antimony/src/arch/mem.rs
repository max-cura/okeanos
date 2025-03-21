//! Memory subsystem on the C906. Implements cache operations.

use crate::arch::{Mcor, Mhcr, Mhint, mcor, mhcr, mhint};

/// Enable the data cache. Note that it is necessary to enable the data cache if we wish to be able
/// to use atomics; this is because LR/SC work by setting monitors on the L1 D-cache.
///
/// Also enables D-cache prefetching.
pub fn enable_dcache() {
    mhcr::set_bits(
        Mhcr(0).with_de(true), // enable dcache
    );
    mhint::set_bits(
        Mhint(0).with_dpld(true), // enable dcache prefetch
    );
}

/// Enable the instruction cache. Also enables branch prediction, branch target prediction, return
/// stack, I-cache prefetching, and I-cache way prediction.
pub fn enable_icache() {
    mhcr::set_bits(
        Mhcr(0)
            .with_ie(true) // enable icache
            .with_bpe(true) // enable branch prediction
            .with_btb(true) // enable branch target prediction
            .with_rs(true), // enable return stack
    );
    mhint::set_bits(
        Mhint(0)
            .with_ipld(true) // enable icache prefetch
            .with_iwpe(true), // enable icache way prediction
    );
}

/// Write all dirty entries out of the D-cache. **Note that this operation is not synchronous!**
/// In order to synchronize the end of this operation, you must call [`dcache_write_out_sync`].
/// For a convenient function that provides both operations, use [`dcache_write_out_and_sync`].
#[inline]
pub fn dcache_write_out() {
    mcor::set_bits(Mcor(0).with_cache_sel_d(true).with_clr(true))
}
/// Waits for the operation begun by [`dcache_write_out`] to complete and then returns.
/// This function does not initialize a write-out operation!
/// For a convenient function that provides both operations, use [`dcache_write_out_and_sync`].
#[inline]
pub fn dcache_write_out_sync() {
    while mcor::read().clr() {}
}
/// Combined operation of [`dcache_write_out`] and [`dcache_write_out_sync`].
#[inline]
pub fn dcache_write_out_and_sync() {
    dcache_write_out();
    dcache_write_out_sync();
}

/// Invalidate all entries in the D-cache. **Note that this operation is not synchronous!**
/// In order to synchronize the end of this operation, you must call [`dcache_invalidate_sync`].
/// For a convenient function that provides both operations, use [`dcache_invalidate_and_sync`].
pub fn dcache_invalidate() {
    mcor::set_bits(Mcor(0).with_cache_sel_d(true).with_inv(true))
}
/// Waits for the operation begun by [`dcache_invalidate`] to complete and then returns.
/// This function does not initialize an invalidation operation!
/// For a convenient function that provides both operations, use [`dcache_invalidate_and_sync`].
pub fn dcache_invalidate_sync() {
    while mcor::read().inv() {}
}
/// Combined operation of [`dcache_invalidate`] and [`dcache_invalidate_sync`].
pub fn dcache_invalidate_and_sync() {
    dcache_invalidate();
    dcache_invalidate_sync();
}

/// Invalidate all entries in the I-cache, the branch history tables, and the branch target buffers.
/// **Note that this operation is not synchronous!** In order to synchronize the end of this
/// operation, you must call [`icache_invalidate_sync`]. For a convenient function that provides
/// both operations, use [`icache_invalidate_and_sync`].
pub fn icache_invalidate() {
    mcor::set_bits(
        Mcor(0)
            .with_cache_sel_i(true)
            .with_inv(true) // invalidate icache
            .with_bht_inv(true) // invalidate branch history tables (branch prediction)
            .with_btb_inv(true), // invalidate branch target buffers (branch target prediction)
    )
}
/// Waits for the operation begun by [`icache_invalidate`] to complete and then returns.
/// This function does not initialize an invalidation operation!
/// For a convenient function that provides both operations, use [`icache_invalidate_and_sync`].
pub fn icache_invalidate_sync() {
    while {
        let r = mcor::read();
        r.inv() || r.bht_inv() || r.btb_inv()
    } {}
}
/// Combined operation of [`icache_invalidate`] and [`icache_invalidate_sync`].
pub fn icache_invalidate_and_sync() {
    icache_invalidate();
    icache_invalidate_sync();
}

/// `fence.i` instruction.
///
/// Synchronizes instruction and data streams. Ensures that a _subsequent_ instruction fetch on a
/// hart will see any _previous_ data stores already visible to the _same_ hart. (Note that this
/// does not provide any cross-hart synchronization).
pub fn fence_i() {
    unsafe { ::core::arch::asm!("fence.i") }
}
/// `fence` instruction. `fence!(predecessor, successor)`.
///
/// No external device/coprocessor/hart can observe any operation in the successor set following a
/// `FENCE` before any operation in the predecessor set preceding the `FENCE`.
///
/// Usage:
/// ```rs
/// fence!(io, r);
/// ```
pub macro fence($pre:ident, $post:ident) {
    #[allow(unused_unsafe)]
    unsafe {
        ::core::arch::asm!(concat!("fence ", stringify!($pre), ", ", stringify!($post)))
    }
}
