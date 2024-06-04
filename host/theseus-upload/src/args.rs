use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(clap::ValueEnum, Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum TraceLevel {
    #[default]
    Off,
    Control,
    All,
}
impl Display for TraceLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceLevel::Off => write!(f, "off"),
            TraceLevel::Control => write!(f, "control"),
            TraceLevel::All => write!(f, "all"),
        }
    }
}

#[derive(clap::ValueEnum, Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum Protocol {
    #[default]
    Theseus,
    CS240LX,
}
impl Display for Protocol {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Theseus => write!(f, "theseus"),
            Protocol::CS240LX => write!(f, "cs240lx"),
        }
    }
}

#[derive(clap::Parser, Debug, Clone)]
#[command(version, about, long_about=None)]
pub struct Args {
    /// Address to load file to
    #[arg(short, long, default_value_t=0x8000, value_parser=clap_num::maybe_hex::<u32>)]
    pub(crate) address: u32,

    /// USB device to write to; will try to autodetect if not specified
    #[arg(short, long)]
    pub(crate) device: Option<PathBuf>,

    /// Baud rate to use
    #[arg(short, long, default_value_t = 115200)]
    pub(crate) baud: u32,

    /// Increase message verbosity
    #[arg(short='v', action=clap::ArgAction::Count)]
    pub(crate) verbose: u8,

    /// Silence all output,
    #[arg(short, long)]
    pub(crate) quiet: bool,

    /// Microsecond timestamping for debugging timing issues
    #[arg(long)]
    pub(crate) timestamps: bool,

    /// Controls dumping of messages
    #[arg(short, long, default_value_t)]
    pub(crate) trace: TraceLevel,

    // /// Bootloader protocol to use
    // #[arg(short, long, default_value_t)]
    // pub(crate) protocol: Protocol,
    /// .bin file to write
    #[arg(required = true)]
    pub(crate) bin_file: PathBuf,
}
