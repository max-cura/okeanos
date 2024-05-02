//! Various linker-defined symbols that point to Important Things that our program cares about.

extern "C" {
    static __okns_code_start__: u8;
    static __okns_code_end__: u8;

    static __okns_rodata_start__: u8;
    static __okns_rodata_end__: u8;

    static __okns_data_start__: u8;
    static __okns_data_end__: u8;

    static __okns_bss_start__: u8;
    static __okns_bss_end__: u8;

    static __okns_prog_end__: u8;
}