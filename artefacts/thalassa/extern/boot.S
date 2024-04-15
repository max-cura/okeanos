@ vim:ft=armv6
@
@ FILE artefacts/thalassa/extern/boot.S
@ DESC System entry point upon boot.
@

#define SUPER_MODE 0b10011
#define MODE_MASK 0b11111

#define STACK_ADDR 0x8000000

#define EXTERN_KERNEL_FN __tlss_kernel_init
#define EXTERN_REBOOT_FN __tlss_fast_reboot

@ Op1=0, Rd=reg, CRn=c7, CRm=c5, Op2=4 : Flush prefetch buffer
#define _prefetch_flush(reg)     \
    mov reg, #0;                \
    mcr p15, 0, reg, c7, c5, 4

.extern EXTERN_KERNEL_FN
.extern EXTERN_REBOOT_FN

.section ".text.boot"

.globl _start
_start:
    @ enter supervisor mode, disable IRQs, flush prefetch
    @ disable IRQs (A2-11)
    mrs r0, cpsr @ 8000
    and r0, r0, #(~MODE_MASK) @ 8004
    orr r0, r0, #SUPER_MODE @ 8008
    orr r0, r0, #(1 << 7) @ 800c
    msr cpsr, r0 @ 8010
    mov r1, #0 @ 8014
    mcr p15,0,r1,c7,c5,4 @ 8018

    mov sp, #STACK_ADDR
    mov fp, #0
    bl EXTERN_KERNEL_FN
    bl EXTERN_REBOOT_FN