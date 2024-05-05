/*
Defined symbols:

__symbol_exec_start__

__symbol_code_start__
__symbol_code_end__
__symbol_rodata_start__
__symbol_rodata_end__
__symbol_data_start__
__symbol_data_end__
__symbol_bss_start__
__symbol_bss_end__

__symbol_exec_end__
 */

SECTIONS {
    /* 0x8000 is a sort of standardized location at which stuff is loaded.
     * Being as such, it is the location that our bootloader(s) default to.
     */
    .text 0x8000 : {
        __symbol_exec_start__ = .;
        __symbol_code_start__ = .;
        KEEP(*(.text.boot))
        *(.text*)
        __symbol_code_end__ = .;
        . = ALIGN(8);
    }
    .rodata : {
        __symbol_rodata_start__ = .;
        *(.rodata*)
        __symbol_rodata_end__ = .;
    }
    .data : {
        __symbol_data_start__ = .;
        *(.data*)
        . = ALIGN(4);
        __symbol_data_end__ = .;
    }
    /* VERY IMPORTANT NOTE: The executable will zero out every byte between __symbol_bss_start__ and __okns_bss_end__!!!
     */
    .bss : {
        . = ALIGN(4);
        __symbol_bss_start__ = .;
        *(.bss*)
        /* anything that wasn't explicitly assigned a section */
        *(COMMON)
        . = ALIGN(8);
        __symbol_bss_end__ = .;
        . = ALIGN(8);
        __symbol_exec_end__ = .;
    }
}