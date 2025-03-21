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

__symbol_stack_begin__
__symbol_stack_init__

__symbol_exec_end__
 */

SECTIONS {
    ENTRY(_start)
    .text 0x8000 : {
        __symbol_exec_start__ = .;
        __symbol_code_start__ = .;
        KEEP(*(.text.start))
        *(.text*)
        __symbol_code_end__ = .;
        . = ALIGN(8);
    }
    .rodata : {
        . = ALIGN(0x1000);
        __symbol_rodata_start__ = .;
        *(.rodata*)
        __symbol_rodata_end__ = .;
    }
    .data : {
        . = ALIGN(0x1000);
        __symbol_data_start__ = .;
        *(.data*)
        . = ALIGN(4);
        __symbol_data_end__ = .;
    }
    /* VERY IMPORTANT NOTE: The executable will zero out every byte between __symbol_bss_start__ and __okns_bss_end__!!!
     */
    .bss : {
        . = ALIGN(0x1000);
        . = ALIGN(4);
        __symbol_bss_start__ = .;
        *(.bss*)
        /* anything that wasn't explicitly assigned a section */
        *(COMMON)
        . = ALIGN(8);
        __symbol_bss_end__ = .;
    }
    .stack : {
        . = ALIGN(8);
        __symbol_stack_begin__ = .;
        . = . + 0x20000;
        __symbol_stack_init__ = .;
        . = ALIGN(8);
    }
    /* loadable section */
    .data.args ALIGN(256) (TYPE=SHT_PROGBITS) : ALIGN(256) {
        . = ALIGN(4);
        __symbol_args_begin__ = .;
        BYTE(0);
        . = . + 0xff;
        __symbol_args_end__ = .;
        . = ALIGN(4);
        __symbol_exec_end__ = .;
    }
}