@ vim:ft=arm
@
@ FILE extern/thread.S
@ DESC Thread functions
@

.globl _bis_yield_from_super
.globl _bis_resume

.extern _bis_switch_from_super

@ pub extern "C" _bis_yield_from_super()
_bis_yield_from_super:
    mrc p15, 0, r0, c13, c0, 4 @TPIDRPRW
    mov r1, r0
    mrs r3, cpsr
    mov r2, r14
    stmia r0!, {r0-r14}
    stmia r0, {r2, r3}
    mov r0, r1
    b _bis_switch_from_super

@ pub extern "C" _bis_resume(th: *mut ThreadHeader) -> !
_bis_resume:
    ldr r1, [r0, #64]
@    and r2, r1, #31
@    cmp r2, #0x10
@    bne _bis_resume.priv
@_bis_resume.user:
@    @ not totally sure why this uses r14=lr?
@    @ [r0] through [r0, #64] is the register file
@    mov r14, r0
@    ldmia r14, {r0,r1,r2,r3,r4,r5,r6,r7,r8,r9,r10,r11,r12,r13,r14}^
@    @ now pointing to (resume-addr, spsr)
@    add r14, r14, #60
@    rfeia r14
_bis_resume.priv:
    msr spsr, r1
    ldmia r0, {r0,r1,r2,r3,r4,r5,r6,r7,r8,r9,r10,r11,r12,r13,r14,r15}^