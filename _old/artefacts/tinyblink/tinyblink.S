@ vim:ft=arm
@
@ FILE artefacts/tinyblink/tinyblink.S
@ DESC system test binary
@

#define SUPER_MODE 0b10011
#define MODE_MASK 0b11111

@ note that this is 0x0800 0000 not 0x8000 0000
#define STACK_ADDR 0x08000000

#define EXTERN_KERNEL_FN __theseus_init
#define EXTERN_REBOOT_FN __theseus_reboot

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
    @ originally:
    @mov r0, #SUPER_MODE
    @ changed:
    mrs r0, cpsr
    and r0, r0, #(~MODE_MASK)
    orr r0, r0, #SUPER_MODE
    @ disable IRQs (A2-11)
    orr r0, r0, #(1 << 7)
    msr cpsr, r0
    _prefetch_flush(r1)

_init_gpio:
    ldr r10, loc.gpio_base

    ldr r5, [r10, #0x08]
    mov r7, #7
    mvn r7, r7, lsl #21
    and r5, r5, r7
    mov r7, #1
    orr r5, r5, r7, lsl #21
    str r5, [r10, #0x08]

    ldr r5, [r10, #0x10]
    mov r7, #7
    mvn r7, r7, lsl #21
    and r5, r5, r7
    mov r7, #1
    orr r5, r5, r7, lsl #21
    str r5, [r10, #0x10]

    bl _systmr_read_low
    mov r1, r0
    mov r2, #1
_loop:
    bl _systmr_read_low
    bl _systmr_elapsed
    cmp r0, #(1<<20)
    bge _toggle
    b _loop
_toggle:
    cmp r2, #1
    beq _toggle_on

    bl _led27_off
    @bl _led47_off
    mov r2, #1

    b _toggle_done
_toggle_on:
    bl _led27_on
    @bl _led47_on
    mov r2, #0
_toggle_done:
    bl _systmr_read_low
    mov r1, r0
    bl _led47_on
    b _loop

#define SYSTMR_CHI 0x8
#define SYSTMR_CLO 0x4

_led27_on:
    mov r7, #1
    lsl r7, r7, #27
    str r7, [r10, #0x1c]
    bx lr
_led27_off:
    mov r7, #1
    lsl r7, r7, #27
    str r7, [r10, #0x28]
    bx lr
_led47_on:
    mov r7, #1
    lsl r7, r7, #(47-32)
    str r7, [r10, #0x2c]
    bx lr
_led47_off:
    mov r7, #1
    lsl r7, r7, #(47-32)
    str r7, [r10, #0x20]
    bx lr

@ CLOBBERS: r9
@ RETURNS: r0
_systmr_read_low:
    ldr r9, loc.systmr_base
    ldr r0, [r9, #SYSTMR_CLO]
    bx lr

@ ARGUMENTS: r1, r0
@ DESCRIPTION: r0 - r1
@ RETURNS: r0
_systmr_elapsed:
    sub r0, r0, r1
    bx lr

loc.systmr_base: .word 0x20003000
loc.gpio_base: .word 0x20200000

