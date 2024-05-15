pub fn decode_branch_target(opcode: u32, opcode_ptr: *const u32) -> Option<*const u32> {
    if (opcode & 0xf000_000) != 0xe000_0000 {
        // Conditional branch makes no sense
        None
    } else if (opcode & 0x0f00_0000) != 0x0a00_0000 {
        // Only accept the `B` instruction
        None
    } else {
        let signed_imm24 = opcode & 0x00ff_ffff;
        let sign_bit = signed_imm24 & 0x0080_0000;
        let sign_extension = (!(sign_bit - 1)) & 0xff00_0000;
        let sign_extended = (signed_imm24 | sign_extension) as i32 as isize;
        let pc = unsafe { opcode_ptr.byte_offset(8) };
        let address = unsafe { pc.byte_offset(sign_extended) };
        Some(address)
    }
}
pub fn encode_branch(opcode_ptr: *const u32, target: *const u32) -> Option<u32> {
    let pc = unsafe { opcode_ptr.byte_offset(8) };
    let diff = unsafe { target.byte_offset_from(pc) };
    let diff_shifted = (diff >> 2) as usize;
    let signed_imm24 = diff_shifted & 0x00ff_ffff;
    // ensure that we can represent the number in 24 bits
    let re_extended = {
        let sign_bit = signed_imm24 & 0x0080_0000;
        let sign_extension = (!(sign_bit - 1)) & 0xff00_0000;
        signed_imm24 | sign_extension
    };
    if re_extended != signed_imm24 {
        None
    } else {
        let encoded = 0xea00_0000 | (signed_imm24 as u32);
        Some(encoded)
    }
}

// #[derive(Copy, Clone, Debug)]
// pub struct __B {
//     opcode: u32
// }
// impl __B {
//     pub unsafe fn new_for_unchecked(
//         opcode_ptr: *const u32,
//         target: *const u32,
//     ) -> Option<Self> {
//         encode_branch(opcode_ptr, target).map(|opcode| Self { opcode })
//     }
//     pub fn opcode(&self) -> u32 { self.opcode }
//     pub fn decode_target(self: Pin<&Self>) -> Option<*const u32> {
//         decode_branch_target(self.opcode, core::ptr::addr_of!(self.get_ref().opcode))
//     }
// }
// impl Debug for __B {
//     fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
//         if let Some(ptr) = self.decode_address() {
//             write!(f, "asm(b {:p})", ptr)
//         } else {
//             write!(f, "asm(<invalid (b)>)")
//         }
//     }
// }
