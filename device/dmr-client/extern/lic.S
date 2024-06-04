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

__bis__lic_handle_fiq:
    subs pc, r14, #4

__bis__lic_handle_smc:
    movs pc, r14

__bis__lic_handle_bkpt:
    subs pc, r14, #4