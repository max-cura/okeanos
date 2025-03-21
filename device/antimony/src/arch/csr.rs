#[macro_export]
macro_rules! define_csr {
    ($name:ident $(: $cooked:ty)? $(,$uns:tt)?) => {
        $crate::define_csr!($name => $name $(: $cooked)? $(,$uns)?);
    };
    ($cname:expr => $name:ident $(: $cooked:ty)? $(,$uns:tt)?) => {
    pub mod $name {
        pub mod raw {
            pub unsafe fn read() -> usize {
                let ret: usize;
                unsafe { ::core::arch::asm!(concat!("csrr {}, ", stringify!($cname)), out(reg) ret) };
                ret
            }
            pub unsafe fn write(val: usize) {
                unsafe { ::core::arch::asm!(concat!("csrw ", stringify!($cname), ", {}"), in(reg) val) };
            }
            pub unsafe fn clear_bits(val: usize) {
                unsafe { ::core::arch::asm!(concat!("csrc ", stringify!($cname), ", {}"), in(reg) val) };
            }
            pub unsafe fn set_bits(val: usize) {
                unsafe { ::core::arch::asm!(concat!("csrs ", stringify!($cname), ", {}"), in(reg) val) };
            }
            pub unsafe fn read_write(val: usize) -> usize {
                let ret: usize;
                unsafe { ::core::arch::asm!(concat!("csrrw {}, ", stringify!($cname), ", {}"), out(reg) ret, in(reg) val) }
                ret
            }
            pub unsafe fn read_set(val: usize) -> usize {
                let ret: usize;
                unsafe { ::core::arch::asm!(concat!("csrrs {}, ", stringify!($cname), ", {}"), out(reg) ret, in(reg) val) }
                ret
            }
            pub unsafe fn read_clear(val: usize) -> usize {
                let ret: usize;
                unsafe { ::core::arch::asm!(concat!("csrrc {}, ", stringify!($cname), ", {}"), out(reg) ret, in(reg) val) }
                ret
            }
            pub unsafe fn modify(f: impl FnOnce(usize) -> usize) {
                let initial = unsafe { self::read() };
                let new = f(initial);
                unsafe { self::write(new) };
            }
        }
        $(
        pub fn read() -> $cooked {
            <$cooked>::from(unsafe { raw::read() })
        }
        pub fn write(val: $cooked) {
            unsafe { raw::write(val.into()) }
        }
        pub fn clear_bits(val: $cooked) {
            unsafe { raw::clear_bits(val.into()) }
        }
        pub fn set_bits(val: $cooked) {
            unsafe { raw::set_bits(val.into()) }
        }
        pub fn read_write(val: $cooked) -> $cooked {
            unsafe { raw::read_write(val.into()) }.into()
        }
        pub fn read_set(val: $cooked) -> $cooked {
            unsafe { raw::read_set(val.into()) }.into()
        }
        pub fn read_clear(val: $cooked) -> $cooked {
            unsafe { raw::read_clear(val.into()) }.into()
        }
        pub fn modify(f: impl FnOnce($cooked) -> $cooked) {
            unsafe { raw::modify(|x| f(x.into()).into()) }
        }
        )?
    }
}
}

pub use define_csr;
