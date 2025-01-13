#![feature(iter_intersperse)]

mod tty;
mod upload;

use clap::{CommandFactory, Parser};
use std::ffi::OsStr;
use std::fs::DirEntry;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use tracing_subscriber::util::SubscriberInitExt;

pub const DEFAULT_LOAD_ADDRESS: u32 = 0x8000;

fn main() {
    color_eyre::install().expect("Failed to install `color_eyre`");
    tracing_subscriber::fmt::Subscriber::builder()
        .without_time()
        .finish()
        .init();

    let args = parse_args();

    if let Err(e) = upload::upload(args) {
        tracing::error!("failed to upload: {e}");
        std::process::exit(1);
    }
}

pub enum FormatDetails {
    Bin { load_address: u32 },
    Elf,
}

pub struct Args {
    device: PathBuf,
    baud: u32,
    quiet: bool,
    file: PathBuf,
    format_details: FormatDetails,
}

fn parse_args() -> Args {
    let mut args = CmdArgs::parse();

    let device = args.device.unwrap_or_else(|| {
        tracing::warn!("no device specified, searching for suitable TTY");
        if let Some(most_recent_device) = find_most_recent_tty() {
            tracing::info!("using device {}", most_recent_device.display());
            most_recent_device
        } else {
            tracing::error!("failed to find suitable TTY device");
            tracing::error!(
                "expected device in /dev like one of: {}",
                PATTERNS
                    .iter()
                    .map(|p| p.to_string() + "*")
                    .intersperse(", ".to_string())
                    .collect::<String>()
            );
            std::process::exit(1);
        }
    });

    let extension_hint = args
        .file
        .extension()
        .map(OsStr::to_ascii_lowercase)
        .map(|os_str| {
            if os_str == "bin" {
                Some(ObjectType::Bin)
            } else if os_str == "elf" {
                Some(ObjectType::Elf)
            } else {
                None
            }
        })
        .flatten();
    let object_type = args.override_object_type.or(extension_hint);
    let Some(object_type) = object_type else {
        CmdArgs::command()
            .error(
                clap::error::ErrorKind::ValueValidation,
                "file must be .elf or .bin, or --override-object-type must be specified",
            )
            .exit();
    };

    if object_type == ObjectType::Bin {}
    let format_details = match object_type {
        ObjectType::Elf => {
            if args.load_address.is_some() {
                CmdArgs::command()
                    .error(
                        clap::error::ErrorKind::ArgumentConflict,
                        "--load-address can only be specified if file is a bin",
                    )
                    .exit();
            }
            FormatDetails::Elf
        }
        ObjectType::Bin => {
            let load_address = args.load_address.unwrap_or_else(|| {
                tracing::warn!(
                    "no load address specified for object of type BIN, using default load address of {:x}",
                    DEFAULT_LOAD_ADDRESS
                );
                DEFAULT_LOAD_ADDRESS
            });
            FormatDetails::Bin { load_address }
        }
    };

    Args {
        device,
        baud: args.baud,
        quiet: args.quiet,
        file: args.file,
        format_details,
    }
}

static PATTERNS: [&str; 6] = [
    "ttyUSB",
    "ttyACM",
    "tty.usbserial",
    "cu.usbserial",
    "tty,SLAB_USB",
    "cu.SLAB_USB",
];
fn find_most_recent_tty() -> Option<PathBuf> {
    let Some(dev) = std::fs::read_dir("/dev").ok() else {
        tracing::error!("Failed to open /dev");
        return None;
    };
    dev.filter_map(|entry| -> Option<(DirEntry, std::fs::Metadata)> {
        entry.ok().and_then(|e| {
            let metadata = e.metadata().ok()?;

            let ft = e.file_type().ok()?;
            if !ft.is_char_device() {
                return None;
            };

            let path_buf = e.path();
            let file_name = path_buf.file_name()?;
            if !PATTERNS.iter().any(|pattern| {
                std::str::from_utf8(file_name.as_bytes())
                    .map(|f| f.starts_with(pattern))
                    .unwrap_or(false)
            }) {
                return None;
            };
            Some((e, metadata))
        })
    })
    .max_by_key(|(_, m)| m.modified().expect("Metadata::modified() not available"))
    .map(|(e, _)| e.path().to_path_buf())
}

#[derive(clap::ValueEnum, Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum ObjectType {
    #[default]
    Elf,
    Bin,
}

#[derive(clap::Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct CmdArgs {
    /* General settings
     */
    /// USB device to write to; will try to autodetect if not specified
    #[arg(short, long)]
    pub(crate) device: Option<PathBuf>,

    /// Baud rate to upload at
    #[arg(short, long, default_value_t = 115200)]
    pub baud: u32,

    // /// Increase message verbosity
    // #[arg(short='v', action=clap::ArgAction::Count)]
    // pub verbose: u8,
    /// Silence all output
    #[arg(short, long)]
    pub quiet: bool,

    #[arg(required = true)]
    pub file: PathBuf,

    #[arg(long)]
    pub override_object_type: Option<ObjectType>,

    /* filetype: .bin
     */
    #[arg(short, long)]
    pub load_address: Option<u32>,
}
