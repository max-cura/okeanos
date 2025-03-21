pub mod uart;
// pins are arranged into banks of size 0x30
// +0  CFG0 CFG1 CFG2
// +10 DAT
// +14 DRV0 DRV1 DRV2
// +24 PULL0 PULL1 PULL2

macro_rules! define_pinmux {
    { $( $pin:ident : { $( $f_n:literal => $name:ident ),* $(,)? } ),* $(,)? } => {
        pub mod pin {
        $(
            #[repr(u32)]
            #[derive(Debug, Copy, Clone, Eq, PartialEq, ::core::marker::ConstParamTy)]
            #[allow(non_camel_case_types)]
            pub enum $pin {
                Input = 0x0,
                Output = 0x1,
                Disable = 0xf,
                $($name = $f_n),*
            }
        )*
        }
        #[derive(Debug, Copy, Clone, Eq, PartialEq, ::core::marker::ConstParamTy)]
        pub enum PinId {
            $( $pin ),*
        }
        #[derive(Debug, Copy, Clone, Eq, PartialEq, ::core::marker::ConstParamTy)]
        pub enum PinFunc {
            $( $pin($crate::peripherals::pin::$pin) ),*
        }
        impl PinId {
            const fn const_eq(self, rhs: Self) -> bool {
                match (self, rhs) {
                    $(
                    (Self::$pin, Self::$pin) => true,
                    )*
                    _ => false
                }
            }
            const fn not_found(self) -> ! {
                match self {
                    $(
                    Self::$pin => panic!(concat!("pin ", stringify!($pin), " is already assigned")),
                    )*
                }
            }
        }
        struct ConvToPinFunc<const P: PinId>;
        #[const_trait]
        trait ToPinFunc {
            type Func;
            fn convert(func: Self::Func) -> PinFunc;
        }
        $(
            impl const ToPinFunc for ConvToPinFunc<{ PinId::$pin }> {
                type Func = $crate::peripherals::pin::$pin;
                fn convert(func: Self::Func) -> PinFunc {
                    PinFunc::$pin(func)
                }
            }
        )*
        impl PinFunc {
            pub fn pin_id(self) -> PinId {
                match self {
                    $(
                        Self::$pin(..) => PinId::$pin,
                    )*
                }
            }
        }
        pub const PIN_COUNT : usize = $ {count($pin)};

        const PINSET_INITIAL : [POpt; PIN_COUNT] = [
            $( POpt::Some(PinId::$pin) ),*
        ];

        $(
        impl Pin<{ PinId::$pin }> {
            pub const fn get_func<const F: $crate::peripherals::pin::$pin>() -> PinFunc {
                ConvToPinFunc::<{ PinId::$pin }>::convert(F)
            }
            pub fn select<const F: $crate::peripherals::pin::$pin>(self) -> PinBound<{ Self::get_func::<F>() }> {
                let (bank_no, bank_offset, cfg_offset) = const {
                    let pin_name = stringify!($pin);
                    let bank_no = (pin_name.as_bytes()[1] - b'A') as usize;
                    assert!(1 <= bank_no && bank_no <= 6);
                    let pin_no = if pin_name.len() > 3 {
                        (pin_name.as_bytes()[2] - b'0') * 10 + (pin_name.as_bytes()[3] - b'0')
                    } else {
                        pin_name.as_bytes()[2] - b'0'
                    } as usize;
                    let bank_offset = pin_no / 8;
                    let cfg_offset = pin_no % 8;
                    (bank_no, bank_offset, cfg_offset)
                };

                // now we can write to the config register
                let cfg_reg_addr = 0x0200_0000usize + bank_no * 0x30 + 0x4 * bank_offset;
                let cfg_reg_ptr : *mut u32 = core::ptr::with_exposed_provenance_mut(cfg_reg_addr);

                let v_init = unsafe { cfg_reg_ptr.read_volatile() };
                let v_next = v_init & (!(0xf << (cfg_offset * 4))) | ((F as u32) << (cfg_offset * 4));
                unsafe { cfg_reg_ptr.write_volatile(v_next) };

                PinBound(sealed::Token)
            }
        }
        $(
            impl PinBound<{ PinFunc::$pin($crate::peripherals::pin::$pin::$name) }> {
                pub fn revert(self) -> Pin<{ PinId::$pin }> {
                    Pin(sealed::Token)
                }
            }
        )*
        )*

    }
}

define_pinmux! {
    PB0: { 6 => UART0_TX, 7 => UART2_TX },
    PB1: { 6 => UART0_RX, 7 => UART2_RX },
    PB8: { 6 => UART0_TX, 7 => UART1_TX },
    PB9: { 6 => UART0_RX, 7 => UART1_RX },
    PD10: { 5 => UART3_TX },
    PD11: { 5 => UART3_RX },
    PD21: { 4 => UART2_TX },
    PD22: { 4 => UART2_RX },
}

mod sealed {
    #[derive(Eq, PartialEq)]
    pub struct Token;
}

pub struct Pin<const P: PinId>(sealed::Token);
pub struct PinBound<const P: PinFunc>(sealed::Token);

#[derive(Eq, PartialEq, ::core::marker::ConstParamTy)]
pub enum POpt {
    Some(PinId),
    None,
}
#[derive(Eq, PartialEq)]
pub struct Pinmux<const PS: Pins>(sealed::Token);
impl<const PS: Pins> Pinmux<PS> {
    pub const fn assign<const P: PinId>(self) -> (Pin<P>, Pinmux<{ PS.take(P) }>) {
        (Pin(sealed::Token), Pinmux::<{ PS.take(P) }>(sealed::Token))
    }
}
pub unsafe fn forge_pinmux() -> Pinmux<PINS_INITIAL> {
    Pinmux(sealed::Token)
}

#[derive(Eq, PartialEq, ::core::marker::ConstParamTy)]
pub struct Pins([POpt; PIN_COUNT]);
pub const PINS_INITIAL: Pins = Pins(PINSET_INITIAL);
impl Pins {
    pub const fn take(mut self, pin: PinId) -> Pins {
        let mut i = 0;
        let found = loop {
            if i >= PIN_COUNT {
                break false;
            }
            if matches!(self.0[i], POpt::Some(p) if pin.const_eq(p)) {
                self.0[i] = POpt::None;
                break true;
            }
            i += 1;
        };
        if !found {
            pin.not_found();
        }
        self
    }
}

#[macro_export]
macro_rules! assign_pins {
    {
        $pinmux:ident;
        $( let $pin:ident : $t:ident; )+
    } => {
        $(
            let ($pin, $pinmux) : ($crate::peripherals::Pin<{ $crate::peripherals::PinId::$t }>, _) = $pinmux.assign();
        )+
    }
}
pub macro pin {
    ($pi:ident) => {
        { $crate::peripherals::PinId::$pi }
    },
    ($pi:ident as $pf:ident) => {
        { $crate::peripherals::PinFunc::$pi($crate::peripherals::pin::$pi::$pf) }
    }
}
