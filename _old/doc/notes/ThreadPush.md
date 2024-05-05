Enabling the MMU:
1. program all relevant CP15 registers
2. program 1st- and 2nd- level descriptor page tables
3. disable and invalidate icache
4. enable MMU using CP15 control register
5. (re-enable) icache

Disabling the MMU:
1. disable dcache in CP15 control register
	1. must disable dcache in corresponding world before, or at the same time as, disabling the MMU
2. disable MMU in CP15 control register

> Note: TLB contents are preserved across MMU disable

Memory region attributes: 6.6 pg. 6-15

TexRemap=1

Primary Region Remap Register `p15, 0, <Rd>, c10, c2, 0`
Normal Memory Region Register `p15, 0, <Rd>, c10, c2, 1`
(pg 3-101)

Table 6-5 on pg. 6-17

Shared Normal memory:
- not L1-cached
- TCM memory cannot be shared
- Writes to Shared Normal memory might not be atomic.
- Reads to Shared Normal memory that are aligned to access size are atomic.

Page table format (ARMv6 format)
256 32-bit entries (4KB)
page type by examining bits (1:0) of the second level descriptor
00 for first and second level descriptors = unmapped with upper 30 bits ignored

bits:
- nG Not-Global
- S Shared (Normal only)
- XN Execute Never
- APX

First level:
```
[31:2 ignored] 00 - translation fault
[31:2 reserved] 11 - translation fault
coarse page table:
[31:10 base addr] P [8:5 domain] 0 NS 0 01
section:
[31:20 base addr] NS 0 nG S APX [14:12 TEX] [11:10 AP] P [8:5 domain] XN C B 10
supersection:
[31:24 base addr] [23:20 SBZ] NS 1 nG S APX [14:12 TEX] [11:10 AP] P [8:5 ignored (domain=0)] XN C B 10
```
(P bit not supported on 1176jzf-s)
Second level:
```
[31:2 ignored] 00 - translation fault
large page (64K)
[31:16 base addr] XN [14:12 TEX] nG S APX [8:6 SBZ] [5:4 AP] C B 01
extended small page (4K)
[31:12 base addr] nG S APX [8:6 TEX] [5:4 AP] CB 1 XN
```

to prevent alias problems where cache sizes greater than 16KB have been implemented, you must restrict the mapping of pages that remap virtual address bits (13:12)
- for icache, the Isize P bit, bit(11) of cp15 c0, indicates if this is necessary
- for dcache, the Dsize P bit, bit(23) of cp15 c0, indicates if this is necessary
bits 13:12 of virtual addresses mapped to the same physical address must be equal
bits 13:12 of virtual addresses must be equal to bits 13:12 of their mapped physical address UNLESS all page sizes are equal to 4KB

Or, set CZ flag in p15,c1 AuxCtlReg, though this will limit all caches to 16KB

