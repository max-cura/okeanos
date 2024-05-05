/*
 * DEFINED SYMBOLS:
 *  __tlss_code_start__      : start of the .code section (should be 0x8000)
 *  __tlss_code_end__        : end of the .code section, NOT ALIGNED UP
 *  __tlss_rodata_start__    : start of the .rodata section
 *  __tlss_rodata_end__      : end of the .rodata section, NOT ALIGNED UP
 *  __tlss_data_start__      : start of the .data section
 *  __tlss_data_end__        : end of the .data section, 4-aligned
 *  __tlss_bss_start__       : start of the .bss section, 4-aligned
 *  __tlss_bss_end__         : end of the .bss section, 8-aligned
 *  __tlss_prog_end__        : end of program data, 8-aligned
 *  __tlss_heap_start__      : beginning of the heap
 *
 * Note that we currently have __tlss_bss_end__ = __tlss_prog_end__ = __tlss_heap_start
 */

SECTIONS {
    /* 0x8000 is the standard location at which "stuff" is loaded
     * I can't say I completely understand the boot process we're currently using, but! it loads to 0x8000
     * Just don't know if that's the 0x8000 from firmware/config.txt, ARMBASE 0x8000 from a UART bootloader, a boot
     * mode, or somewhere else.
     */
    .text 0x8000 : {
        __tlss_code_start__ = .;
        /* since .text.boot isn't referenced from anywhere, and we have --gc-sections, we need to KEEP it so that it
         * doesn't get discarded
         */
        KEEP(*(.text.boot))
        *(.text*)
        __tlss_code_end__ = .;
        . = ALIGN(8);
    }
    .rodata : {
        __tlss_rodata_start__ = .;
        *(.rodata*)
        __tlss_rodata_end__ = .;
    }
    .data : {
        __tlss_data_start__ = .;
        *(.data*)
        . = ALIGN(4);
        __tlss_data_end__ = .;
    }
    /* Note to self: BSS aka "Better Save Space" contains statically allocated variables that have not been initialized
     * Always wondered what it was for
     */
    .bss : {
        . = ALIGN(4);
        __tlss_bss_start__ = .;
        *(.bss*)
        /* anything that wasn't explicitly assigned a section */
        *(COMMON)
        . = ALIGN(8);
        __tlss_bss_end__ = .;
        /* I'm pretty sure this line is redundant? */
        . = ALIGN(8);
        __tlss_prog_end__ = .;
        __tlss_heap_start__ = .;
    }
}