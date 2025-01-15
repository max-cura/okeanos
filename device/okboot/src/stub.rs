unsafe extern "C" {
    static __symbol_exec_start__: [u8; 0];
    static __symbol_code_start__: [u8; 0];
    static __symbol_code_end__: [u8; 0];
    static __symbol_rodata_start__: [u8; 0];
    static __symbol_rodata_end__: [u8; 0];
    static __symbol_data_start__: [u8; 0];
    static __symbol_data_end__: [u8; 0];
    static __symbol_bss_start__: [u8; 0];
    static __symbol_bss_end__: [u8; 0];
    static __symbol_exec_end__: [u8; 0];
}

unsafe extern "C" {
    static __symbol_relocation_stub: [u8; 0];
    static __symbol_relocation_stub_end: [u8; 0];
}

pub fn locate_end() -> *const [u8; 0] {
    unsafe { &raw const __symbol_exec_end__ }
}
