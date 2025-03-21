#![feature(exit_status_error)]

use clap::{Args, Parser, Subcommand};
use console::Term;
use eyre::Result;
use fancy_duration::AsFancyDuration;
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Output, exit};
use std::str::FromStr;
use tracing_subscriber::filter::EnvFilter;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Debug, Clone, Subcommand)]
#[command(about)]
enum Task {
    Build(BuildTask),
    Run(RunTask),
}
impl Task {
    fn run(self) -> Result<()> {
        match self {
            Task::Build(build_task) => {
                build_task.run()?;
                Ok(())
            }
            Task::Run(run_task) => run_task.run(),
        }
    }
}

#[derive(Debug, Clone, Args)]
struct RunTask {
    #[arg(required = true)]
    name: String,
    #[arg(short = 'd', long = "device")]
    device: Option<String>,
}
impl RunTask {
    fn run(self) -> Result<()> {
        let elf_file = BuildTask {
            name: self.name.clone(),
        }
        .run()?;
        let mut args = vec![elf_file.as_path()];
        if let Some(device) = &self.device {
            args.push(Path::new("-d"));
            args.push(Path::new(device));
        }
        if let Err(e) = duct::cmd("okdude", args)
            .env("RUST_LOG", "okdude=info")
            .unchecked()
            .run()
            .expect("unchecked command produced error")
            .status
            .exit_ok()
        {
            tracing::error!(
                "Failed to upload {:?}: okdude ({e})",
                console::style(elf_file.file_name().unwrap()).green()
            )
        } else {
            tracing::info!(
                "Successfully ran {:?}",
                console::style(elf_file.file_name().unwrap()).green()
            )
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Args)]
struct BuildTask {
    #[arg(required = true)]
    name: String,
}

impl BuildTask {
    fn run(self) -> Result<PathBuf> {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let term = Term::stderr();
        let metadata = cargo_metadata::MetadataCommand::new().no_deps().exec()?;
        let package = metadata
            .packages
            .iter()
            .find(|p| p.name == self.name)
            .ok_or_else(|| eyre::eyre!("no such package: {}", self.name))?;
        let crate_dir = package
            .manifest_path
            .ancestors()
            .nth(1)
            .unwrap()
            .as_std_path();
        let config_toml = crate_dir
            .ancestors()
            .filter(|p| p.join(".cargo/config.toml").is_file())
            .next()
            .expect("No .cargo/config.toml detected")
            .join(".cargo/config.toml");
        let config_toml_value: toml::Value =
            toml::from_str(&std::fs::read_to_string(config_toml)?)?;
        let target_str = config_toml_value
            .get("build")
            .unwrap()
            .get("target")
            .unwrap()
            .as_str()
            .unwrap();
        build_crate(&self.name, &cargo, &term, crate_dir)?;
        let lib_file = metadata
            .target_directory
            .as_std_path()
            .join(target_str)
            .join("release")
            .join(format!("lib{}.a", package.targets[0].name));
        let target: Target = Target::from_str(target_str)?;
        let linker_script = {
            let local_override = crate_dir.join(target_str).with_extension("ld");
            if local_override.is_file() {
                local_override
            } else {
                metadata
                    .workspace_root
                    .as_std_path()
                    .join("infra")
                    .join(target_str)
                    .with_extension("ld")
            }
        };
        let build_dir = metadata
            .workspace_root
            .join("build")
            .join(&self.name)
            .as_std_path()
            .to_path_buf();
        if !build_dir.is_dir() {
            tracing::debug!("create dir {}", build_dir.display());
            std::fs::create_dir_all(&build_dir)?;
        }
        let elf_file = link(
            &build_dir,
            &self.name,
            lib_file.as_path(),
            target,
            linker_script.as_path(),
            &term,
        )?;
        Ok(elf_file)
    }
}

#[derive(Debug, Copy, Clone)]
enum Target {
    Armv6zk,
    Rv64gc,
}
impl FromStr for Target {
    type Err = eyre::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "armv6zk-none-eabihf" => Ok(Self::Armv6zk),
            "riscv64gc-unknown-none-elf" => Ok(Self::Rv64gc),
            x => {
                eyre::bail!("unknown target {x}")
            }
        }
    }
}

fn linker_flags_for_target(target: Target) -> Vec<OsString> {
    let mut base = vec!["-z", "noexecstack", "-nostdlib"];
    match target {
        Target::Armv6zk => {
            base.push("-mcpu=arm1176jzf-s");
            base.push("-march=armv6zk+fp");
            base.push("-mfpu=vfpv2");
            base.push("-mfloat-abi=hard");

            base.push("-nostartfiles");
            base.push("-Wl,--gc-sections");
            base.push("-ffreestanding");
        }
        Target::Rv64gc => {
            base.push("--gc-sections");
        }
    }
    base.into_iter().map(OsString::from).collect()
}

fn linker_for_target(target: Target) -> &'static str {
    match target {
        Target::Armv6zk => "arm-none-eabi-ld",
        Target::Rv64gc => "riscv64-unknown-elf-ld",
    }
}

fn link(
    build_dir: &Path,
    name: &str,
    library: &Path,
    target: Target,
    linker_script: &Path,
    _term: &Term,
) -> Result<PathBuf> {
    let linker = linker_for_target(target);
    tracing::info!("`{linker}`");
    let elf_path = build_dir.join(name).with_extension("elf");
    let mut args: Vec<OsString> = vec![
        OsString::from("-T"),
        linker_script.as_os_str().to_os_string(),
    ];
    args.extend(linker_flags_for_target(target));
    args.push(library.as_os_str().to_os_string());
    args.push(OsString::from("-o"));
    args.push(elf_path.as_os_str().to_os_string());
    let expr = duct::cmd(linker, args).stderr_to_stdout();
    let start = chrono::Utc::now();
    let (_line_count, output) = run_expr(&expr)?;
    if let Err(e) = output.status.exit_ok() {
        tracing::error!("linking failed ({}).", e);
        exit(e.into_raw());
    } else {
        tracing::info!(
            "Successfully linked {:?} in {}",
            console::style(elf_path.file_name().unwrap()).green(),
            (chrono::Utc::now() - start).fancy_duration().to_string(),
        );
        Ok(elf_path)
    }
}

fn build_crate(name: &str, cargo: &str, _term: &Term, dir: &Path) -> Result<()> {
    tracing::info!("`cargo build -p {name}`");
    let expr = duct::cmd!(
        cargo,
        "build",
        "-p",
        name,
        "--color",
        "always",
        "--profile",
        "release"
    )
    .dir(dir);
    let start = chrono::Utc::now();
    let (_line_count, output) = run_expr(&expr)?;
    if let Err(e) = output.status.exit_ok() {
        tracing::error!("cargo build failed ({}).", e);
        exit(e.into_raw());
    } else {
        tracing::info!(
            "Successfully built {} in {}",
            console::style(name).green(),
            (chrono::Utc::now() - start).fancy_duration().to_string(),
        );
        Ok(())
    }
}

fn run_expr(expr: &duct::Expression) -> Result<(usize, Output)> {
    let reader = expr.unchecked().stderr_to_stdout().reader()?;
    let mut buf_reader = BufReader::new(reader);
    let line_count = (&mut buf_reader)
        .lines()
        .inspect(|l| {
            if let Ok(l) = l {
                eprintln!("{}", l)
            }
        })
        .count();
    let reader = buf_reader.into_inner();
    let output = reader
        .try_wait()
        .expect("try_wait() on unchecked ReaderHandle returned an Err")
        .expect("try_wait() on unchecked ReaderHandle returned an Ok(None)");
    Ok((line_count, output.clone()))
}

fn main() -> Result<()> {
    color_eyre::install().expect("failed to install color-eyre");
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .map_event_format(|f| f.without_time())
        .init();

    let cli = Cli::parse();

    cli.command.run()
}
