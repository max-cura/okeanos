#![allow(incomplete_features)]
#![feature(adt_const_params)]
#![feature(generic_const_exprs)]

use std::marker::ConstParamTy;

#[derive(Debug, Copy, Clone, Eq, PartialEq, ConstParamTy)]
#[repr(u32)]
enum Alt {
    Input = 0b000,
    Output = 0b001,
    Alt0 = 0b100,
    Alt1 = 0b101,
    Alt2 = 0b110,
    Alt3 = 0b111,
    Alt4 = 0b011,
    Alt5 = 0b010,
}

trait RegVal: Copy {
    fn from_repr(v: u32) -> Self;
    fn as_repr(self) -> u32;
}
trait Register {
    type Value: RegVal;
    fn write(&mut self, v: Self::Value);
    fn read(&mut self) -> Self::Value;
    fn modify<F: FnOnce(Self::Value) -> Self::Value>(&mut self, f: F) {
        let v = f(self.read());
        self.write(v);
    }
}

trait True {}
struct If<const C: bool> {}
impl True for If<true> {}

struct Pin<const P: usize, const ALT: Alt>;

#[derive(Copy, Clone)]
struct FselV<const B: usize>(u32);
impl<const B: usize> RegVal for FselV<B> {
    fn from_repr(v: u32) -> Self {
        Self(v)
    }
    fn as_repr(self) -> u32 {
        self.0
    }
}
impl<const B: usize> FselV<B>
where
    If<{ B <= 5 }>: True,
{
    fn pin<const P: usize, const ALT: Alt>(&mut self) -> Pin<P, ALT>
    where
        If<{ (P / 10) == B }>: True,
    {
        self.0 &= !(7 << ((P % 10) * 3));
        self.0 |= (ALT as u32) << ((P % 10) * 3);
        Pin
    }
}
#[derive(Copy, Clone)]
struct SetV<const B: usize>(u32);
impl<const B: usize> RegVal for SetV<B> {
    fn from_repr(v: u32) -> Self {
        Self(v)
    }

    fn as_repr(self) -> u32 {
        self.0
    }
}
impl<const B: usize> SetV<B>
where
    If<{ B < 2 }>: True,
{
    fn set<const P: usize>(&mut self, _pin: &mut Pin<P, { Alt::Output }>)
    where
        If<{ (P / 32) == B }>: True,
    {
        self.0 = self.0 | (1 << (P % 32));
    }
}
#[derive(Copy, Clone)]
struct ClrV<const B: usize>(u32);
impl<const B: usize> RegVal for ClrV<B> {
    fn from_repr(v: u32) -> Self {
        Self(v)
    }

    fn as_repr(self) -> u32 {
        self.0
    }
}
impl<const B: usize> ClrV<B>
where
    If<{ B < 2 }>: True,
{
    fn clear<const P: usize>(&mut self, _pin: &mut Pin<P, { Alt::Output }>)
    where
        If<{ (P / 32) == B }>: True,
    {
        // println!("set {:08x}", 1 << (P % 32));
        self.0 = self.0 | (1 << (P % 32));
    }
}
#[derive(Copy, Clone)]
struct LevV<const B: usize>(u32);
impl<const B: usize> RegVal for LevV<B> {
    fn from_repr(v: u32) -> Self {
        Self(v)
    }

    fn as_repr(self) -> u32 {
        self.0
    }
}
impl<const B: usize> LevV<B>
where
    If<{ B < 2 }>: True,
{
    fn is_set<const P: usize>(&self, _pin: &mut Pin<P, { Alt::Input }>) -> bool
    where
        If<{ (P / 32) == B }>: True,
    {
        (self.0 & (1 << (P % 32))) != 0
    }
}

trait Gpio {
    type Fsel<const B: usize>: Register<Value = FselV<B>>;
    fn fsel<const B: usize>(&self) -> Self::Fsel<B>;

    type Set<const B: usize>: Register<Value = SetV<B>>;
    fn set<const B: usize>(&self) -> Self::Set<B>;

    type Clr<const B: usize>: Register<Value = ClrV<B>>;
    fn clr<const B: usize>(&self) -> Self::Clr<B>;

    type Lev<const B: usize>: Register<Value = LevV<B>>;
    fn lev<const B: usize>(&self) -> Self::Lev<B>;
}

use core::cell::RefCell;
use core::marker::PhantomData;
use std::rc::Rc;

struct FakeRegister<V: RegVal> {
    bank: Rc<RefCell<Inner>>,
    _pd: PhantomData<V>,
}
impl<V: RegVal> FakeRegister<V> {
    fn new(bank: Rc<RefCell<Inner>>) -> Self {
        Self {
            bank,
            _pd: PhantomData::default(),
        }
    }
}
impl<const B: usize> Register for FakeRegister<FselV<B>> {
    type Value = FselV<B>;

    fn write(&mut self, v: Self::Value) {
        println!(
            "writing {1:#010x} to {0:#010x}",
            0x2020_0000 + 4 * B,
            // match B {
            //
            //     0 => 0x2020_0028,
            //     1 => 0x2020_002c,
            //     _ => unreachable!()
            // },
            v.as_repr()
        );
        self.bank.borrow_mut().fsel_registers[B] = v.as_repr();
    }

    fn read(&mut self) -> Self::Value {
        let v = self.bank.borrow().fsel_registers[B];
        println!("reading {v:#010x} from {}", 0x2020_0000 + 4 * B);
        Self::Value::from_repr(v)
    }
}
impl<const B: usize> Register for FakeRegister<SetV<B>> {
    type Value = SetV<B>;

    fn write(&mut self, v: Self::Value) {
        if B == 0 {
            if (v.as_repr() & (1 << 26)) != 0 {
                self.bank.borrow_mut().p2610 = true;
            }
        }
        println!(
            "writing {1:#010x} to {0:#010x}",
            match B {
                0 => 0x2020_001c,
                1 => 0x2020_0020,
                _ => unreachable!(),
            },
            v.as_repr()
        )
    }

    fn read(&mut self) -> Self::Value {
        unreachable!()
    }
}
impl<const B: usize> Register for FakeRegister<ClrV<B>> {
    type Value = ClrV<B>;

    fn write(&mut self, v: Self::Value) {
        if B == 0 {
            if (v.as_repr() & (1 << 26)) != 0 {
                self.bank.borrow_mut().p2610 = false;
            }
        }
        println!(
            "writing {1:#010x} to {0:#010x}",
            match B {
                0 => 0x2020_0028,
                1 => 0x2020_002c,
                _ => unreachable!(),
            },
            v.as_repr()
        )
    }

    fn read(&mut self) -> Self::Value {
        unreachable!()
    }
}
impl<const B: usize> Register for FakeRegister<LevV<B>> {
    type Value = LevV<B>;

    fn write(&mut self, _v: Self::Value) {
        unreachable!()
    }

    fn read(&mut self) -> Self::Value {
        let v = if B == 0 && self.bank.borrow().p2610 {
            1 << 10
        } else {
            0
        };
        println!(
            "reading {1} from {:#010x}",
            match B {
                0 => 0x2020_0034,
                1 => 0x2020_0038,
                _ => unreachable!(),
            },
            (v & (1 << 10)) != 0
        );
        LevV(v)
    }
}
struct Inner {
    fsel_registers: [u32; 6],
    p2610: bool,
}

struct FakeGpio {
    inner: Rc<RefCell<Inner>>,
}
impl Gpio for FakeGpio {
    type Fsel<const B: usize> = FakeRegister<FselV<B>>;

    fn fsel<const B: usize>(&self) -> Self::Fsel<B> {
        FakeRegister::new(Rc::clone(&self.inner))
    }

    type Set<const B: usize> = FakeRegister<SetV<B>>;

    fn set<const B: usize>(&self) -> Self::Set<B> {
        FakeRegister::new(Rc::clone(&self.inner))
    }

    type Clr<const B: usize> = FakeRegister<ClrV<B>>;

    fn clr<const B: usize>(&self) -> Self::Clr<B> {
        FakeRegister::new(Rc::clone(&self.inner))
    }

    type Lev<const B: usize> = FakeRegister<LevV<B>>;

    fn lev<const B: usize>(&self) -> Self::Lev<B> {
        FakeRegister::new(Rc::clone(&self.inner))
    }
}

fn main() {
    let fake_gpio = FakeGpio {
        inner: Rc::new(RefCell::new(Inner {
            fsel_registers: [0, 0x00012000, 0, 0, 0, 0],
            p2610: false,
        })),
    };
    let mut fsel0 = fake_gpio.fsel::<0>();
    let mut f0 = FselV(0);
    let mut p9 = f0.pin::<9, { Alt::Output }>();
    fsel0.write(f0);

    let mut fsel1 = fake_gpio.fsel::<1>();
    let mut f1 = FselV(0);
    let mut p10 = f1.pin::<10, { Alt::Input }>();
    f1.pin::<14, { Alt::Alt5 }>();
    f1.pin::<15, { Alt::Alt5 }>();
    fsel1.write(f1);

    let mut fsel2 = fake_gpio.fsel::<2>();
    let mut f2 = FselV(0);
    let mut p26 = f2.pin::<26, { Alt::Output }>();
    fsel2.write(f2);

    let mut p0 = f0.pin::<0, { Alt::Output }>();
    fsel0.write(f0);

    let mut set0 = fake_gpio.set::<0>();
    let mut lev0 = fake_gpio.lev::<0>();
    let mut clr0 = fake_gpio.clr::<0>();

    {
        let mut c0 = ClrV(0);
        c0.clear(&mut p26);
        clr0.write(c0);
    }

    let mut p9on = false;
    for _ in 0..5 {
        let lev10 = lev0.read().is_set(&mut p10);
        if lev10 {
            let mut s0 = SetV(0);
            s0.set(&mut p0);
            set0.write(s0);
        } else {
            let mut c0 = ClrV(0);
            c0.clear(&mut p0);
            clr0.write(c0);
        }
        p9on = !p9on;
        if p9on {
            let mut s0 = SetV(0);
            s0.set(&mut p9);
            set0.write(s0);
            let mut s0 = SetV(0);
            s0.set(&mut p26);
            set0.write(s0);
        } else {
            let mut c0 = ClrV(0);
            c0.clear(&mut p9);
            clr0.write(c0);
            let mut c0 = ClrV(0);
            c0.clear(&mut p26);
            clr0.write(c0);
        }
    }
}
