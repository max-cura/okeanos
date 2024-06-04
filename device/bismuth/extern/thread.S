@ vim:ft=arm

@
.extern __bis_switch_context

.globl __bis_yield_from_irq
.globl __bis_yield_from_swi
.globl __bis_yield_from_generic_super
.globl __bis_switch_back_priv
.globl __bis_switch_back_user

__bis_yield_from_irq:
    @ mode is IRQ, interrupts are disabled

    @ Store r14 (LR) and SPSR underneath the IRQ-mode stack pointer
    @ note that this does NOT modify the IRQ-mode stack pointer
    srsdb #0b10010
    mrc p15, 0, r14, c13, c0, 4 @ TPIDRPRW

    @ register file is stored at offset 0 in the TCB

    @ First, store registers r0-r12; r13 and r14 can't be stored directly since
    @ we're not sure which execution mode was interrupted. This will instead be
    @ handled in __bis_switch_context
    stmia r14!, {r0, r1, r2, r3, r4, r5, r6, r7, r8, r9, r10, r11, r12}
    add r14, r14, #8
    @ store return address and spsr
    ldmdb sp, {r0, r1}
    stmia r14!, {r0, r1}

    @ disable interrupts and switch to System mode
    cpsid i, #31

    @ set up arguments for __bis_switch_context
    sub r0, r14, #(16*4 + 4)
    mov r1, #1 @ magic value: IRQ mode
    bl __bis_switch_context
    @ unreachable

__bis_yield_from_swi:
    @ mode is Supervisor, interrupts are disabled

    @ Store r14 (LR) and SPSR underneath the SVC-mode stack pointer
    @ note that this does NOT modify the SVC-mode stack pointer
    srsdb #0b10011
    mrc p15, 0, r14, c13, c0, 4 @TPIDRPRW

    stmia r14!, {r0, r1, r2, r3, r4, r5, r6, r7, r8, r9, r10, r11, r12}
    @ We're not able to set the W bit, so manually increase r14
    stmia r14, {r13, r14}^
    add r14, r14, #8
    ldmdb sp, {r0, r1}
    stmia r14!, {r0, r1}

    @cpsid i, #31

    sub r0, r14, #(16*4+4)
    mov r1, #2 @ magic value: yield_from_user
    b __bis_switch_context
    @ unreachable

__bis_yield_from_generic_super:
    @ mode is Supervisor, interrupts are ???

    mrc p15, 0, r0, c13, c0, 4 @TPIDRPRW
    ldr r1, =.ctx_return
    mrs r2, cpsr
    mov r3, r0
    stmia r0!, {r0, r1, r2, r3, r4, r5, r6, r7, r8, r9, r10, r11, r12, r13, r14}
    stmia r0, {r1, r2}
    mov r0, r3
    mov r1, #3 @ magic value: yield from kernel thread
    b __bis_switch_context
.ctx_return:
    bx lr

@ __bis_switch_back_user(*mut ThreadControlBlock) -> !
__bis_switch_back_user:
    mov r14, r0
    ldmia r14, {r0,r1,r2,r3,r4,r5,r6,r7,r8,r9,r10,r11,r12,r13,r14}^
    add r14, r14, #60
    rfeia r14

    @mov r3, r0
    @add r0, r0, #60
    @@ load {PC, SPSR}
    @ldmia r0, {r1, r2}
    @@ldr r1, [r0, #60] @ r15
    @@ldr r2, [r0, #64] @ spsr
    @sub sp, sp, #8
    @stmia sp, {r1, r2}
    @add r0, r3, #52
    @stm r0, {r13, r14}^
    @ldmia r3, {r0,r1,r2,r3,r4,r5,r6,r7,r8,r9,r10,r11,r12}
    @rfeia sp!

@ __bis_switch_back(*mut ThreadControlBlock)
__bis_switch_back_priv:
    @ mode is System, interrupts are disabled.
    @ r0 points to a TCB
    ldr r1, [r0, #64]
    msr spsr, r1
    ldmia r0, {r0, r1, r2, r3, r4, r5, r6, r7, r8, r9, r10, r11, r12, r13, r14, r15}^
