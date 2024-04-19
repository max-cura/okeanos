@ vim:ft=arm
@
@ FILE artefacts/theseus-device/extern/boot.S
@ DESC System entry point upon boot.
@

#define SUPER_MODE 0b10011
#define MODE_MASK 0b11111

#define STACK_ADDR 0x8000000

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

#if 0
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
