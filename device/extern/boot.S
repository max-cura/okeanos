@ vim:ft=arm
@
@ FILE extern/boot.S
@ DESC Generic system entry point
@

#ifndef CUSTOM_STACK
.extern __symbol_exec_start__
    #define STACK_HIGH =__symbol_exec_start__
#elif !defined(STACK_HIGH)
    #error "CUSTOM_STACK is defined but STACK_HIGH is not"
#endif

#ifndef CUSTOM_SYMBOLS
    #define EXTERN_KERNEL_FN __symbol_kstart
    #define EXTERN_REBOOT_FN __symbol_kreboot
#elif !defined(EXTERN_KERNEL_FN) || !defined(EXTERN_REBOOT_FN)
    #error "CUSTOM_SYMBOLS is defined but one of EXTERN_KERNEL_FN or EXTERN_REBOOT_FN is not"
#endif
.extern EXTERN_KERNEL_FN
.extern EXTERN_REBOOT_FN

#define SUPER_MODE 0b10011
#define MODE_MASK 0b11111

@ Op1=0, Rd=reg, CRn=c7, CRm=c5, Op2=4 : Flush prefetch buffer
#define _prefetch_flush(reg)     \
    mov reg, #0;                \
    mcr p15, 0, reg, c7, c5, 4

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

    ldr sp, STACK_HIGH
    mov fp, #0
    bl EXTERN_KERNEL_FN
    bl EXTERN_REBOOT_FN
.loop: b .loop