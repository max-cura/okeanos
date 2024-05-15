@ vim:ft=arm
@
@ FILE bismuth/extern/lic.S
@ DESC Legacy Interrupt Controller support routines. Provides default (stub) implementations for all of the (defined)
@      vectors.
@

.globl __bis__lic_handle_reset_default
.globl __bis__lic_handle_undefined_instruction_default
.globl __bis__lic_handle_software_interrupt_default
.globl __bis__lic_handle_prefetch_abort_default
.globl __bis__lic_handle_data_abort_default
.globl __bis__lic_handle_irq_default
.globl __bis__lic_handle_fiq_default
.globl __bis__lic_handle_smc_default
.globl __bis__lic_handle_bkpt_default

__bis__lic_handle_reset_default:
    // ...

__bis__lic_handle_undefined_instruction:
    movs pc, r14

__bis__lic_handle_software_interrupt:
    movs pc, r14

__bis__lic_handle_prefetch_abort:
    subs pc, r14, #4

__bis__lic_handle_data_abort:
    subs pc, r14, #8

__bis__lic_handle_irq_default:
    subs pc, r14, #4

__bis__lic_handle_fiq_default:
    subs pc, r14, #4

__bis__lic_handle_smc_default:
    movs pc, r14

__bis__lic_handle_bkpt_default:
    subs pc, r14, #4