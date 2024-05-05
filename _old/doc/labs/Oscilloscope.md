cycles:
- 700MHz

generator:
- 1MHz PWM signal (from 500MHz clock with divi=500 divf=0)
- serial mode, RNG1=32 FIF1=0x=5555_5555 RPTL1=1

scope:
- pins 0-27
- FIQ
- 1MiB

MMU config:
- Set buffer as write-through no allocate unshared normal memory

Buffer fmt - always write 32 out
```
| CYC | MSK | MSK_LEV | (CYC2) |
^
\- 16-byte aligned
```

General interrupts:
- control bank: 7e00b000 (2000b000)
- FIQ control: 20C
	- bit 7: FIQ enable ; set 1
	- bits 6:0: source ; (49)

Vector table base address:
- `MCR p15, 0, <Rd>, c12, c0, 0`, must be 32-byte aligned
- want V=0 (???)
- FIQ will use NSBA + 0x1c

GPIO interrupts:
- one per bank, and one across both banks
- four interrupts: 0,1,2,3 (49-52)
	- 1 is bank 0
	- 1,2 are bank 1 (mirrored)
	- 3 is both banks
