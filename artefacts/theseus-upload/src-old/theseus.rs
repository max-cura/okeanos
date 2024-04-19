use serialport::TTYPort;
use theseus_common::theseus::TheseusVersion;
use crate::args::Args;
use crate::bin_name;
use color_eyre::eyre;

mod v1;

pub(crate) fn version_dispatch(ver: TheseusVersion, args: &Args, tty: &mut TTYPort) -> eyre::Result<()> {
    log::info!("[{}]: Using {} protocol", bin_name(), ver);

    match ver {
        TheseusVersion::TheseusV1 => {
            v1::dispatch(args, tty)
        }
    }
}
