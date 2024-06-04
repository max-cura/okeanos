#![feature(iter_intersperse)]
#![feature(ascii_char)]

pub mod args;
pub mod echo;
pub mod find_tty;
mod hexify;
pub mod io;
pub mod legacy;
pub mod theseus;
mod tty;
pub mod upload;

use crate::args::Args;
use clap::CommandFactory;
use clap::Parser;
use std::ffi::OsStr;
use std::process::exit;
use std::sync::OnceLock;

use color_eyre::eyre;
use stderrlog::Timestamp;

static BIN_NAME: OnceLock<String> = OnceLock::new();
pub fn bin_name() -> &'static str {
    BIN_NAME
        .get_or_init(|| {
            <Args as CommandFactory>::command()
                .get_bin_name()
                .unwrap_or({
                    let env_name = std::env!("CARGO_CRATE_NAME");
                    if env_name.is_empty() {
                        "theseus_upload"
                    } else {
                        env_name
                    }
                })
                .to_string()
        })
        .as_str()
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let args = Args::parse();

    stderrlog::new()
        .module(module_path!())
        .quiet(args.quiet)
        .verbosity(args.verbose as usize)
        .timestamp(if args.timestamps {
            Timestamp::Microsecond
        } else {
            Timestamp::Off
        })
        .init()
        .unwrap();

    if !args.bin_file.exists() {
        log::error!("{} does not exist, exiting", args.bin_file.display());
        exit(1);
    }
    if !args.bin_file.is_file() {
        log::error!("{} is not a file, exiting", args.bin_file.display());
        exit(1);
    }
    if !args
        .bin_file
        .extension()
        .map(|e| e == OsStr::new("bin"))
        .unwrap_or(false)
    {
        log::error!("{} is not a .bin file, exiting", args.bin_file.display());
        exit(1);
    }

    upload::protocol_begin(args)
}
