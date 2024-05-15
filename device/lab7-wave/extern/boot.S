@ vim:ft=arm
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

#if 1
@ THIS SECTION (CURRENTLY DEACTIVATED) WAS USED TO FIGURE OUT WHETHER OR NOT
@ THE BOOTLOADER WAS BEING LOADED AT THE RIGHT LOCATION BY start.elf.

@ NOTE: IF SOMEONE ONE DAY REACTIVATES THIS, REASSEMBLE THIS, AND CHECK WHETHER
@       .mark IS STILL AT <_start+0x20>, AND UPDATE loc.mark ACCORDINGLY.
    b .avoid
    @ <_start+0x20>
.mark: .word 0x454d4143
.avoid:

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

    @ pin27
    mov r7, #1
    lsl r7, r7, #27
    str r7, [r10, #0x1c]

    ldr r0, .mark
    ldr r1, loc.mark
    ldr r1, [r1]
    cmp r0, r1
    bne .avoid2
    @ if [0x8020==.mark], then light up 47
    mov r7, #1
    lsl r7, r7, #(47-32)
    str r7, [r10, #0x2c]
.avoid2:
#endif

#if 0
loop$:
    b loop$
#else
    mov sp, #STACK_ADDR
    mov fp, #0
    bl EXTERN_KERNEL_FN
    bl EXTERN_REBOOT_FN
#endif

loc.gpio_base: .word 0x20200000
loc.mark: .word 0x8020

#define TIME_FIQ 0

#define LED_ON(v) \
       mov r8, #0; \
       mcr p15,0,r8,c7,c10,4; \
       mov r8, v; \
       lsl r8, r8, #25; \
       str r8, [r11, #0x1c]; \
       mov r8, #0; \
       mcr p15,0,r8,c7,c10,4
#define LED_OFF \
       mov r8, #0; \
       mcr p15,0,r8,c7,c10,4; \
       mov r8, #0b111; \
       lsl r8, r8, #25; \
       str r8, [r11, #0x28]; \
       mov r8, #0; \
       mcr p15,0,r8,c7,c10,4

.globl _lab6_ivec
.align 5
_lab6_ivec:
    b _lab6_int_unhandled @ 0
    b _lab6_int_unhandled @ 4
    b _lab6_int_unhandled @ 8
    b _lab6_int_unhandled @ c
    b _lab6_int_unhandled @ 10
    b _lab6_int_unhandled @ 14
    b _lab6_int_unhandled @ 18
    b _lab6_fiq_handler @ 1c

.globl _lab6_int_unhandled
_lab6_int_unhandled:
.lop:
    b .lop
.globl _lab6_fiq_handler
_lab6_fiq_handler:
    mrc p15, 0, r8, c15, c12, 1     @ read cycle counter
    mrc p15, 0, sp, c13, c0, 2      @ read buffer pointer

    mov r9, #0                      @ DSB
    mcr p15,0,r9,c7,c10,4
    ldr r9, [r11, #0x40]            @ read EDS0
    ldr r10, [r11, #0x34]           @ read LEV0
    stm sp, {r8, r9, r10}         @ store CYC EDS LEV
    mov r9, #1                      @ clear EDS0
    lsl r9, r9, #13
    str r9, [r11, #0x40]
#if 0
    mov r9, #0                      @ DSB
    mcr p15,0,r9,c7,c10,4 @ DSB
#endif

    @cmp sp, r12
    @beq .fiq_exit
    add sp, sp, #16
.fiq_exit:
    @ write to buffer ptr
    mcr p15, 0, sp, c13, c0, 2
#if TIME_FIQ
    mrc p15, 0, r8, c15, c12, 1
    str r8, [sp, #-4]
#endif
    @ LED_ON(#7)
    @ return
    subs pc, r14, #4

#if 0
.globl _lab6_fiq_handler
_lab6_fiq_handler:
/*
    ldr r8, =0xffffffff
    tst lr,r8
    beq .fret
    @LED_ON
*/
    /*
    @ dsb
    // r8, r9, r10, r11, r12, r13, r14
    mov r9, #0
    mcr p15,0,r9,c7,c10,4
    // fuck it, uart time
    // while !stat.tx_empty() {}
    // [.hex, (lr & 0xf)]
    LED_ON(#1)
.uart_init:
    ldr r9, =0x20215040
    ldr r10, =.hex
    LED_ON(#2)
.uart_wait:
    ldr r8, [r9, #0x24]
    tst r8, #(1<<8)
    beq .uart_wait
    mov r13, #32
    LED_ON(#3)
.uart_nybble:
    sub r13, r13, #4
    mov r8, #0xf
    lsl r8, r8, r11
    and r8, lr, r8
    asr r8, r8, r11
    ldrb r8, [r10, r8]
    str r8, [r9, #0]
    movs r13, r13
    bne .uart_nybble
    LED_ON(#4)
.uart_wait2:
    ldr r8, [r9, #0x24]
    tst r8, #2
    beq .uart_wait2
    mov r8, #0x0a
    str r8, [r9, #0]
    LED_ON(#5)
.uart_done:
    */
    /*
    @ dsb
    mov r9, #0
    mcr p15,0,r9,c7,c10,4
    */
    mrc p15, 0, r8, c15, c12, 1     @ read cycle counter
    mrc p15, 0, sp, c13, c0, 2      @ read buffer pointer

    mov r9, #0; mcr p15,0,r9,c7,c10,4 @ DSB
    ldr r9, [r11, #0x40] @ read EDS0
    ldr r10, [r11, #0x34] @ read LEV0
    stmia sp, {r8, r9, r10}
    mov r9, #1; lsl r9, r9, #13; str r9, [r11, #0x40] @ clear EDS0
    mov r9, #0; mcr p15,0,r9,c7,c10,4 @ DSB

    cmp sp, r12
    beq .fiq_exit
    add sp, sp, #16
.fiq_exit:
    @ write to buffer ptr
    mcr p15, 0, sp, c13, c0, 2
#if TIME_FIQ
    mrc p15, 0, r8, c15, c12, 1
    str r8, [sp, #-4]
#endif
    @ LED_ON(#7)
    @ return
    subs pc, r14, #4
#endif
/*
.hex:
    .ascii "0123456789abcdef"
    */

/*
#if 0
_lab6_fiq_handler:
    mrc p15, 0, r8, c15, c12, 1
    ldr r9, [r11, #0x40] @ EDS0
    ldr r10, [r11, #0x34] @ LEV0
    stmia sp, {r8, r9, r10}
    cmp sp, r12
    beq .fiq_done
    add sp, sp, 16
.fiq_exit:
#if TIME_FIQ
    mrc p15, 0, r8, c15, c12, 1
    str r8, [sp, #-4]
#endif
    subs pc, r14, #4 @ return
.fiq_done:
    @ notify the mainloop
    mov r8, #1
    mcr p15, 0, r8, c13, c0, 2
    b .fiq_exit
#endif
*/