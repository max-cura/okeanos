Hardware boot process.

Initial state:
- supervisor mode, secure world (NS=0)
- peripherals boot in their most secure state

Secure OS at the reset vector must:
1. Initialize Secure OS. Includes normal boot actions:
	1. Generate page tables, switch on MMU
	2. Switch on the stack
	3. Set up run time environment and program stacks for each processor mode
2. Initialize the secure monitor.
	1. Allocate TCM memory for SMon
	2. Allocate scratch memory
	3. Set up SMon stack pointer and initialize its state block
3. Program the partition checker to allocate physical memory available to the Nonsecure OS
4. Yield control to the Nonsecure OS

### Thalassa boot process

1. `_start` in boot.S
	1. disable interrupts
	2. enter SUP mode
	3. jump to `_kernel_init`
2. `_kernel_init`

SEC boot:
1. init UART in polling mode
2. init page tables and MMU
3. init stacks and interrupts
4. init DMA
5. switch UART to DMA mode
6. init SMon
7. init partition checker
8. switch to NONSEC boot
NONSEC boot:
1. init multithreading, futex, etc.
2. filesystem (fcache)

