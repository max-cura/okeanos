//! Various linker-defined symbols that point to Important Things that our program cares about.

extern "C" {
    static __symbol_exec_start__: u8;

    static __symbol_code_start__: u8;
    static __symbol_code_end__: u8;

    static __symbol_rodata_start__: u8;
    static __symbol_rodata_end__: u8;

    static __symbol_data_start__: u8;
    static __symbol_data_end__: u8;

    static __symbol_bss_start__: u8;
    static __symbol_bss_end__: u8;

    static __symbol_exec_end__: u8;
}
