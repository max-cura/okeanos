use crate::steal_println;
use core::fmt::Display;

use quartz::arch::arm1176::cpuid::*;

fn dump_with_label<T: Display + Into<u32> + Copy>(label: &str, f: fn() -> T) {
    let v = f();
    steal_println!("{label}: <{:08x}>\n{v}", Into::<u32>::into(v));
}
fn dump_plain(label: &str, f: fn() -> u32) {
    steal_println!("{label}: <{:08x}>", f());
}

pub fn dump_cpu_info() {
    steal_println!("=== BEGIN CPU Information ===");
    dump_with_label("Main ID", main_id::read);
    dump_with_label("Cache Type", cache_type::read);
    dump_with_label("TCM Status", tcm_status::read);
    dump_with_label("TLB Type", tlb_type::read);
    dump_with_label("Processor Feature 0", processor_feature_0::read);
    dump_with_label("Processor Feature 1", processor_feature_1::read);
    dump_plain("Debug Feature 0", debug_feature_0::read_raw);
    dump_plain("Auxiliary Feature 0", auxiliary_feature_0::read_raw);
    dump_with_label("Memory Model Feature 0", memory_model_feature_0::read);
    dump_with_label("Memory Model Feature 1", memory_model_feature_1::read);
    dump_with_label("Memory Model Feature 2", memory_model_feature_2::read);
    dump_with_label("Memory Model Feature 3", memory_model_feature_3::read);
    dump_with_label("Instruction Set Feature 0", isa_feature_0::read);
    dump_with_label("Instruction Set Feature 1", isa_feature_1::read);
    dump_with_label("Instruction Set Feature 2", isa_feature_2::read);
    dump_with_label("Instruction Set Feature 3", isa_feature_3::read);
    dump_with_label("Instruction Set Feature 4", isa_feature_4::read);
    dump_plain("Instruction Set Feature 5", isa_feature_5::read_raw);
    steal_println!("=== END CPU Information ===");
}
