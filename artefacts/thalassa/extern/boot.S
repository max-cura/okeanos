@ vim:ft=armv6

#define SUPER_MODE 0b10011
#define MODE_MASK 0b11111

#define STACK_ADDR 0x8000000

@ Op1=0, Rd=reg, CRn=c7, CRm=c5, Op2=4 : Flush prefetch buffer
#define _prefetch_flush(reg)     \
    mov reg, #0;                \
    mcr p15, 0, reg, c7, c5, 4

.extern _tlss_kernel_init
.extern _tlss_fast_reboot

.section ".text.boot"

.globl _start
_start:
    @ enter supervisor mode
    @ originally:
    @mov r0, #SUPER_MODE
    @ changed:
    mrs r0, cpsr
    and r0, r0, #(~MODE_MASK)
    orr r0, r0, #SUPER_MODE
    @ disable IRQs (A2-11)
    orr r0, r0, #(1 << 7)
    msr cpsr, r0
    @ why r1?
    @ > I think it's just the first availble register that's not r0?
    _prefetch_flush(r1)

    mov sp, #STACK_ADDR
    mov fp, #0
    bl _tlss_kernel_init
    bl _tlss_fast_reboot

.globl _tlss_arch_put32
@ r1 -> [r0]
_tlss_arch_put32:
    str r1, [r0]
    bx lr

.globl _tlss_arch_get32
@ [r0] -> r0
_tlss_arch_get32:
    ldr r0, [r0]
    bx lr

.globl _tlss_arch_branchto
_tlss_arch_branchto:
    bx r0