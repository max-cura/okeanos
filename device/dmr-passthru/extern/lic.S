@ vim:ft=arm
@
@ FILE bismuth/extern/lic.S
@ DESC Legacy Interrupt Controller support routines. Provides default (stub) implementations for all of the (defined)
@      vectors.
@

.globl __bis__lic_handle_reset
.globl __bis__lic_handle_undefined_instruction
.globl __bis__lic_handle_software_interrupt
.globl __bis__lic_handle_prefetch_abort
.globl __bis__lic_handle_data_abort
.globl __bis__lic_handle_irq
.globl __bis__lic_handle_fiq
.globl __bis__lic_handle_smc
.globl __bis__lic_handle_bkpt

.globl __bis__lic_table
.align 5
__bis__lic_table:
    b __bis__lic_handle_reset
    b __bis__lic_handle_undefined_instruction
    b __bis__lic_handle_software_interrupt
    b __bis__lic_handle_prefetch_abort
    b __bis__lic_handle_data_abort
    nop
    b __bis__lic_handle_irq
    b __bis__lic_handle_fiq

__bis__lic_handle_reset:
    // ...

__bis__lic_handle_undefined_instruction:
    movs pc, r14

.extern __bis_yield_from_swi
.extern __bis_thread_cleanup
__bis__lic_handle_software_interrupt:
    @ r14 points to the first instruction after the SWI

    cmp r0, #0
    ble __bis_yield_from_swi
    cmp r0, #1
    ble __bis_thread_cleanup
    movs pc, r14

loc.gpio_base: .word 0x20200000

__bis__lic_handle_prefetch_abort:
    subs pc, r14, #4

__bis__lic_handle_data_abort:
    subs pc, r14, #8

__bis__lic_handle_irq:
    subs pc, r14, #4

#define LED_ON \
       mov r8, #0; \
       mcr p15,0,r8,c7,c10,4; \
       mov r8, #1; \
       lsl r8, r8, #27; \
       str r8, [r11, #0x1c]; \
       mov r8, #0; \
       mcr p15,0,r8,c7,c10,4

__bis__lic_handle_fiq:
    @ r8, r9, r10, r11, r12, r13, r14
    @ but on the other hand, we have an infinite loop running so /shrug

    @ r13=stack
    @ r12=uart IO reg
    @ r11=gpio base

    @mov r9, #0                      @ DSB
    @mcr p15,0,r9,c7,c10,4
    ldr r10, [r11, #0x34]           @ Read LEV0
    ands r8, r10, #0x0000003f       @ Check if it's addressed to us
    bne .fiq_exit
    and r8, r10, #0x00ff0000        @ Pull out data
    mov r8, r8, ror #16
    mvn r9, #0                      @ Clear EDS0
    str r9, [r11, #0x40]
    mov r9, #0                      @ DSB
    mcr p15,0,r9,c7,c10,4
.uart_wait:                         @ poll MU_STAT
    ldr r9, [r12, #0x24]
    tst r9, #(1<<8)
    beq .uart_wait
    str r8, [r12, #0]               @ *MU_IO = r8
    mov r9, #0                      @ DSB
    mcr p15,0,r9,c7,c10,4
.fiq_exit:
    subs pc, r14, #4

__bis__lic_handle_smc:
    movs pc, r14

__bis__lic_handle_bkpt:
    subs pc, r14, #4