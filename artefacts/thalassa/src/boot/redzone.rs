//! Very stupid redzone checks.
//! Currently duplicates libpi/include/redzone.h

const REDZONE_SIZE : usize = 0x1000;

unsafe fn check_redzone_zeroed() {
    let mut addr = 0;
    while addr < REDZONE_SIZE {
        let v = unsafe { crate::arch::ptr::read_addr32(addr) };
        if v != 0 {
            // safe_panic!("redzone check failed: {}")
        }
        addr += 4;
    }
}

unsafe fn zero_redzone() {
    unsafe {
        core::arch::asm!(
            /* set r0=4092 - it's a bit convoluted, I think bc of the restrictions on immediate
             *               operands? */
            "mov     r0, #1020    ",
            /* set r1=fffffffc - because we have += 4 at the beginning of the loop, it will overflow
             *                   to 0 in the first loop
             */
            "mvn     r1, #3       ",
            "orr     r0, r0, #3072",
            /* set r2=0 */
            "mov     r2, #0       ",
            "2:",
            "add     r1, r1, #4   ",
            "str     r2, [r1]     ",
            "cmp     r1, r0       ",
            /* while "less than", jump to label `2` - the 'b' is for 'backward' */
            "bcc     2b",
        )
    }
}

pub unsafe fn redzone_init() {
    zero_redzone();
    check_redzone_zeroed();
}