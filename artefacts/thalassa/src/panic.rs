use core::sync::atomic::{AtomicU32, Ordering};
use crate::boot::panic_boot::{boot_panic_halt, boot_panic_serial};

const BOOT_HALT : u32 = 0;
const BOOT_DUMP_SERIAL : u32 = 1;

#[repr(u32)]
#[derive(Debug, Copy, Clone)]
pub enum PanicBehaviour {
    BootHalt = BOOT_HALT,
    BootDumpSerial = BOOT_DUMP_SERIAL,
}

impl PanicBehaviour {
    pub const fn init_value() -> Self { Self::BootHalt }
}

impl PanicBehaviour {
    pub fn try_from_u32(b: u32) -> Option<Self> {
        match b {
            0 => Some(Self::BootHalt),
            1 => Some(Self::BootDumpSerial),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct PanicAgent {
    inner: AtomicU32,
}

impl PanicAgent {
    pub const fn new() -> Self {
        Self { inner: AtomicU32::new(PanicBehaviour::init_value() as u32) }
    }

    pub fn set_behaviour(&self, pb: PanicBehaviour) {
        self.inner.store(pb as u32, Ordering::SeqCst);
    }

    pub fn load_behaviour(&self) -> PanicBehaviour {
        let raw = self.inner.load(Ordering::SeqCst);
        unsafe {
            PanicBehaviour::try_from_u32(raw)
                // SAFETY: the only place this is stored is Self::store, and it comes in as a
                // PanicBehaviour
                .unwrap_unchecked()
        }
    }
}

static PANIC_AGENT : PanicAgent = PanicAgent::new();

#[panic_handler]
fn panic(info: &::core::panic::PanicInfo) -> ! {
    let behaviour = PANIC_AGENT.load_behaviour();

    match behaviour {
        PanicBehaviour::BootHalt => boot_panic_halt(info),
        PanicBehaviour::BootDumpSerial => boot_panic_serial(info),
    }
}

pub fn set_panic_behaviour(pb: PanicBehaviour) {
    PANIC_AGENT.set_behaviour(pb)
}