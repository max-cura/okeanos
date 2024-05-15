```
@ pin is 13 (1<<13=8192=0x8000)

.org 0x0
loop:
	ldr r0, [r10, #gpio_reg_EDS0]
	bx r0

.org 0x2000
on_change:
	# EDS0 in r0
	__cp15_cycle_read(r1)
	ldr r2, [r10, #gpio_reg_LEV0]
	stm sp, {r1, r0, r2}
	mov r3, #0
	str r3, [r10, #gpio_reg_EDS0]
	__dsb(r4)
	add sp, sp, #16
	subs pc, lr, #4

irq_timer:
	

@ wherever
start:
	ldr r10, =gpio_base
	b loop

loop2:
	ldrs
```

# IMPORTANT
- are we actually exiting FIQ mode in our current code?