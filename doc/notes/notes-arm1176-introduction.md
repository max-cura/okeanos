#### 1.1 About the processor

Note that the difference between the JZ-S and the JZF-S is solely the VFP coprocessor.

#### 1.2 Extensions to ARMv6

ARM1176JZF-S has:
- `strex/ldrex` for `b,h,d`, and a new Clear Exclusive instruction
- true `nop` and `yield`
- "architectural remap registers"
- cache size restriction (CP15 c1)
- revised use of TEX remap bits (seems to help simplify the page table descriptors?)
- revised use of AP bits: `APX` and `AP[1:0]` encoding `b111` is Privileged or User mode read only access—`AP[0]` indicates an abort type, Access Bit fault, when `CP15 c1[29]` is 1 (???)

#### 1.3 TrustZone security extensions

(not yet of interest)

#### 1.4 ARM1176JZF-S architecture with Jazelle technology

(not interested)

#### 1.5 Components of the processor

There are 15 processing modes:
- User
- Supervisor
- FIQ (fast interrupt)
- IRQ (normal interrupt)
- abort
- system
- undefined
- secure monitor
The first 7 have both Secure or Non Secure world modes.

1.5.3 Prefetch unit

Can fetch from icache, iTCM, or external memory.

1.5.6 Coprocessor interface

LDC, LDCL, STC, STCL, MRC, MRRC, MCR, MCRR, CDP