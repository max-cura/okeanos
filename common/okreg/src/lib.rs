#![feature(trivial_bounds)]
#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(any(test, feature = "std"))]
extern crate std;

tock_registers::peripheral! {
    Gpio {
        0x000 => gpfsel0: Fsel::Register { Read, Write },
    }
}

tock_registers::register_bitfields![
    u32,
    Fsel [
        FSEL0 OFFSET(0) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
        FSEL1 OFFSET(3) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
        FSEL2 OFFSET(6) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
        FSEL3 OFFSET(9) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
        FSEL4 OFFSET(12) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
        FSEL5 OFFSET(15) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
        FSEL6 OFFSET(18) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
        FSEL7 OFFSET(21) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
        FSEL8 OFFSET(24) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
        FSEL9 OFFSET(27) NUMBITS(3) [
            Input = 0,Output = 1,Alt0 = 0b100,Alt1=0b101,Alt2=0b110,Alt3=0b111,Alt4=0b011,Alt5=0b010
        ],
    ]
];

#[cfg(any(test, feature = "std"))]
mod mock {
    use crate::{Fsel, Gpio};
    use std::sync::{Arc, Mutex, OnceLock};
    use tock_registers::{
        ArrayDataType, DataType, Read, Register, ScalarDataType, UIntLike, Write,
    };

    struct Inner {
        words: [u32; 15],
    }
    struct GpioMock {
        inner: Arc<Mutex<Inner>>,
    }
    static INNER: OnceLock<Arc<Mutex<Inner>>> = OnceLock::new();
    fn inner() -> &'static Arc<Mutex<Inner>> {
        INNER.get_or_init(|| Arc::new(Mutex::new(Inner { words: [0; 15] })))
    }
    // struct FakeReg2 {}
    #[derive(Debug, Copy, Clone)]
    struct FakeReg {
        offset: usize,
    }
    impl Register for FakeReg {
        type DataType = Fsel::Register;
    }
    impl Write for FakeReg {
        type LongName = ();

        fn write(&self, value: <<Self as Register>::DataType as DataType>::Value)
        where
            <Self::DataType as DataType>::Value: UIntLike,
        {
            inner().lock().unwrap().words[self.offset] = value;
        }

        unsafe fn write_at_unchecked(
            self,
            index: usize,
            value: <Self::DataType as ArrayDataType>::Element,
        ) where
            Self::DataType: ArrayDataType,
        {
            todo!()
        }
    }
    impl Read for FakeReg {
        fn read(&self) -> <Self::DataType as DataType>::Value
        where
            Self::DataType: ScalarDataType,
        {
            inner().lock().unwrap().words[self.offset]
        }

        unsafe fn read_at_unchecked(self, index: usize) -> <Self::DataType as DataType>::Value
        where
            Self::DataType: ArrayDataType,
        {
            todo!()
        }
    }
    impl GpioMock {}
    impl Gpio for GpioMock {
        type gpfsel0<'s> = FakeReg;

        fn gpfsel0(&self) -> Self::gpfsel0<'_> {
            FakeReg { offset: 0 }
        }
    }
}
