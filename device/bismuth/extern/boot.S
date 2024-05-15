@ vim:ft=arm
@
@ FILE bismuth/extern/boot.S
@ DESC System entry point
@

#define SUPER_MODE 0b10011
#define MODE_MASK 0b11111

@ 2000_0000 is the end of memory (4000_0000 is 1GB)
@ 0800_0000 (orig. value) is.. 128M
@ OS lives in 0..32M, so start stack at 32M=
@ hmm... OS stack protection?
#define STACK_ADDR 0xc000
@#define STACK_ADDR 0x08000000

#define EXTERN_KERNEL_FN __symbol_kstart__
#define EXTERN_REBOOT_FN __symbol_reboot__

@ Op1=0, Rd=reg, CRn=c7, CRm=c5, Op2=4 : Flush prefetch buffer
#define _prefetch_flush(reg)     \
    mov reg, #0;                \
    mcr p15, 0, reg, c7, c5, 4

.extern EXTERN_KERNEL_FN
.extern EXTERN_REBOOT_FN

.section ".text.boot"

.globl _start
_start:
    @ enter supervisor mode
    mrs r0, cpsr
    and r0, r0, #(~MODE_MASK)
    orr r0, r0, #SUPER_MODE
    @ disable IRQs (A2-11)
    orr r0, r0, #(1 << 7)
    msr cpsr, r0
    _prefetch_flush(r1)

    mov sp, #STACK_ADDR
    mov fp, #0
    bl EXTERN_KERNEL_FN
    bl EXTERN_REBOOT_FN
