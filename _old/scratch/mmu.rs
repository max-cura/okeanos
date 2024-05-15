// use core::fmt::{Debug, Formatter};
// use proc_bitfield::bits;
//
// #[repr(u32)]
// #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
// pub enum Domain {
//     Dom0 = 0, Dom1 = 1, Dom2 = 2, Dom3 = 3,
//     Dom4 = 4, Dom5 = 5, Dom6 = 6, Dom7 = 7,
//     Dom8 = 8, Dom9 = 9, Dom10 = 10, Dom11 = 11,
//     Dom12 = 12, Dom13 = 13, Dom14 = 14, Dom15 = 15,
// }
//
// #[repr(u32)]
// #[derive(Copy, Clone, Eq, PartialEq)]
// pub enum Access {
//     PnnUnn = 0x0000_0000,
//     PrwUnn = 0x0000_0400,
//     PrwUro = 0x0000_0800,
//     PrwUrw = 0x0000_0c00,
//     // Note! 0x0000_8000 is RESERVED
//     ProUnn = 0x0000_8400,
//     ProUro = 0x0000_8800,
//     ProUro2 = 0x0000_8c00,
// }
//
// #[repr(transparent)]
// #[derive(Copy, Clone)]
// pub struct TranslationTableEntry(u32);
// impl TranslationTableEntry {
//     pub fn new_page_table(
//         base: *mut PageTable,
//         domain: Domain,
//         ns_bit: bool,
//     ) -> Option<Self> {
//         if !base.is_aligned_to(0xffff_fc00) {
//             None
//         } else {
//             Some(Self(base as usize as u32
//                 | ((domain as u32) << 5)
//                 | ((ns_bit as u32) << 3)
//                 | 1
//             ))
//         }
//     }
//     pub fn new_section(
//         base: *mut u8,
//         ns_bit: bool,
//         not_global: bool,
//         shared: bool,
//         access: Access,
//         tex: Tex,
//         domain: Domain,
//         execute_never: bool,
//         c_bit: bool,
//         b_bit: bool,
//     ) {
//
//     }
// }
// impl Debug for TranslationTableEntry {
//     fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
//         let typ_bits =  self.0 & 3;
//         let section_typ = bits!(self.0, 18);
//         match typ_bits {
//             0 => write!(f, "Armv6TTIgnored({:08x})", self.0),
//             1 => write!(f, "Armv6TTPageTable({:p},P={},dom={},NS={})",
//                 bits!(self.0, u32 @ 10..=31) as usize as *const u8,
//                 bits!(self.0, 9) as u8,
//                 bits!(self.0, 5..=8),
//                 bits!(self.0, 3) as u8,
//             ),
//             2 if !section_typ => {
//                 write!(f, "Armv6TTSection({:p},NS={},nG={},S={},APX={},TEX={},AP={},P={},dom={},XN={},C={},B={}",
//                     (self.0 & 0xfff0_0000) as usize as *const u8,
//                     bits!(self.0, 19) as u8,
//                     bits!(self.0, 17) as u8,
//                     bits!(self.0, 16) as u8,
//                     bits!(self.0, 15) as u8,
//                     bits!(self.0, 12..=14),
//                     bits!(self.0, 10..=11),
//                     bits!(self.0, 9) as u8,
//                     bits!(self.0, 5..=8),
//                     bits!(self.0, 4) as u8,
//                     bits!(self.0, 3) as u8,
//                     bits!(self.0, 2) as u8,
//                 )
//             }
//             2 if section_typ => {
//                 write!(f, "Armv6TTSupersection({:p},NS={},nG={},S={},APX={},TEX={},AP={},P={},dom={},XN={},C={},B={}",
//                     (self.0 & 0xff80_0000) as usize as *const u8,
//                     bits!(self.0, 19) as u8,
//                     bits!(self.0, 17) as u8,
//                     bits!(self.0, 16) as u8,
//                     bits!(self.0, 15) as u8,
//                     bits!(self.0, 12..=14),
//                     bits!(self.0, 10..=11),
//                     bits!(self.0, 9) as u8,
//                     bits!(self.0, 5..=8),
//                     bits!(self.0, 4) as u8,
//                     bits!(self.0, 3) as u8,
//                     bits!(self.0, 2) as u8,
//                 )
//             }
//             3 => {
//                 write!(f, "Armv6TTReserved()")
//             }
//             _ => unreachable!()
//         }
//     }
// }
//
// pub struct TranslationTable {
//     entries: [TranslationTableEntry; 4096],
// }
//
// #[derive(Copy, Clone)]
// pub struct PageTableEntry(u32);
// impl Debug for PageTableEntry {
//     fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
//         let typ_bits =  self.0 & 3;
//         match typ_bits {
//             0 => write!(f, "Armv6PTIgnored({:08x})", self.0),
//             1 => write!(f, "Armv6PTLargePage({:p},XN={},TEX={},nG={},S={},APX={},AP={},C={},B={}",
//                 (self.0 & 0xffff_0000) as usize as *mut u8,
//                 bits!(self.0, 15) as u8,
//                 bits!(self.0, 12..=14),
//                 bits!(self.0, 11) as u8,
//                 bits!(self.0, 10) as u8,
//                 bits!(self.0, 9) as u8,
//                 bits!(self.0, 4..=5),
//                 bits!(self.0, 3) as u8,
//                 bits!(self.0, 2) as u8,
//             ),
//             2 | 3 => write!(f, "Armv6PTSmallPage({:p},nG={},S={},APX={},TEX={},AP={},C={},B={},XN={}",
//                 (self.0 & 0xffff_f000) as usize as *mut u8,
//                 bits!(self.0, 11) as u8,
//                 bits!(self.0, 10) as u8,
//                 bits!(self.0, 9) as u8,
//                 bits!(self.0, 6..=8),
//                 bits!(self.0, 4..=5),
//                 bits!(self.0, 3) as u8,
//                 bits!(self.0, 2) as u8,
//                 bits!(self.0, 0) as u8,
//             ),
//             _ => unreachable!()
//         }
//     }
// }
// pub struct PageTable {
//     entries: [PageTableEntry; 256],
// }