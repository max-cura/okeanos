SECTIONS {
    ENTRY(_start)
    /* SRAM A : 2_0000 - 2_7fff */
    .head : {
        . = 0x20000;
        KEEP(*(.head))
    }
    .text : {
    }
    .rodata : {
    }
    .data : {
    }
    .bss : {
        . = ALIGN(8);
        __symbol_bss_start__ = .;
        . = ALIGN(8);
        __symbol_bss_end__ = .;
    }

    /* SRAM C : 2_8000 - 3_7fff */
    .stack 0x28000 : {
        __symbol_stack_start__ = .;
        . += (128 << 10);
        __symbol_stack_end__ = .;
    }
}