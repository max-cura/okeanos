@ A very small `blink` program

_start:
    mov r0, #1
    mov r1, #0x20000000
    str r0, [r1, +r1, asr #8]!
.L0:
    subs r2, r2, #(1 << 9)
    strmi r0, [r1, #0x1c]
    strpl r0, [r1, #0x28]
    b .L0

