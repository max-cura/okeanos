
SECTIONS {
    ENTRY(_start)
    .text 0x40000000 : {
        PROVIDE(amn_exec_start = .);

        PROVIDE(amn_header_start = .);
        KEEP(*(.head.start))
        KEEP(*(.head.egon))
        PROVIDE(amn_header_end = .);

        PROVIDE(amn_text_start = .);
        *(.text*)
        . = ALIGN(8);
        PROVIDE(amn_text_end = .);
    }
    .rodata : {
        . = ALIGN(8);
        PROVIDE(amn_rodata_start = .);
        *(.rodata*)
        . = ALIGN(8);
        PROVIDE(amn_rodata_end = .);
    }
    .data : {
        __global_pointer$ = . + 0x800;
        . = ALIGN(8);
        PROVIDE(amn_data_start = .);
        *(.data*)
        . = ALIGN(8);
        PROVIDE(amn_data_end = .);
    }
    .bss : {
        . = ALIGN(8);
        PROVIDE(amn_bss_start = .);
        *(.bss*)
        /* anything that wasn't explicitly assigned a section */
        *(COMMON)
        . = ALIGN(8);
        PROVIDE(amn_bss_end = .);
    }
    .stack : {
        PROVIDE(amn_stack_start = .);
        . = . + 0x20000;
        PROVIDE(amn_stack_end = .);

        . = ALIGN(8);
        PROVIDE(amn_exec_end = .);
    }
}