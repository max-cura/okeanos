@ vim:ft=arm
@
@ FILE extern/int.S
@ DESC Interrupt landing pads
@

.section ".text.int"

@ table is:
@ 0         RESET
@ BA+04     UNDEF
@ BA+08     SVC
@ BA+0C     EXTERNAL/INTERNAL PREFETCH
@ BA+10     EXTERNAL/INTERNAL DATA ABORT
@ BA+14     ---
@ BA+18     IRQ
@ BA+1C     FIQ
@ MonBA+8   SMC

.globl _landing_pad_svc
.globl _landing_pad_smc
.globl _landing_pad_undef
.globl _landing_pad_pabt
.globl _landing_pad_fiq
.globl _landing_pad_irq
.globl _landing_pad_dabt
.globl _landing_pad_reset
.globl _landing_pad_bkpt
.globl _landing_pad_none

#define USR 0b10000
#define FIQ 0b10001
#define IRQ 0b10010
#define SUP 0b10011
#define ABO 0b10111
#define UND 0b11011
#define SYS 0b11111

@ (u32, u32, u32, swi: u32, u32) so r0-r2, r3<-sw, stack<-r3
.extern _interrupt_svc
@ () -> () noclobber
.extern _interrupt_irq

_landing_pad_svc:
    sub sp, sp, #8
    srsia #SUP
    push {r3}
    ldr r3, [lr, #-4]
    mvn lr, #(0xff << 24)
    and r3, r3, lr
    bl _interrupt_svc
    add sp, sp, #4
    rfeia sp!

_landing_pad_smc:
    nop
    movs pc, r14

_landing_pad_undef:
    srsdb #UND
    push {r0-r12}
    nop
    pop {r0-r12}
    rfeia sp!
    movs pc, r14

_landing_pad_pabt:
    srsdb #ABO
    push {r0-r12}

    ldr r0, =0x20200000
    mov r1, #(1 << 27)
    str r1, [r0, #0x1c]

    nop
    pop {r0-r12}
    rfeia sp!
    subs pc, r14, #4

_landing_pad_fiq:
    srsdb #FIQ
    push {r0-r12}
    nop
    pop {r0-r12}
    rfeia sp!

_landing_pad_irq:
    sub lr, lr, #4
    stmfd sp!,{r0-r12, lr}
    mrs r0, spsr
    stmfd sp!,{r0}

    sub r0, lr, #4
    bl _interrupt_irq

    ldmia sp!,{r0}
    msr spsr, r0
    ldmia sp!,{r0-r12, pc}^

_landing_pad_dabt:
    srsdb #ABO
    push {r0-r12}

    ldr r0, =0x20200000
    mov r1, #(1 << 27)
    str r1, [r0, #0x1c]

    nop
    pop {r0-r12}
    rfeia sp!

_landing_pad_reset:
    nop
    b _landing_pad_reset

_landing_pad_bkpt:
    nop
    subs pc, r14, #4

_landing_pad_none:
    nop
    b _landing_pad_none
