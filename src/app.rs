use crate::cli::{Cli, Command};
use crate::device::MinidiscDevice;
use crate::netmd::NetMdDevice;
use crate::output::{print_disc_human, print_disc_json};
use crate::udev;
use anyhow::Result;
use std::process::ExitCode;

pub async fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::List { json } => list(json).await,
        Command::Doctor => {
            udev::print_doctor();
            Ok(ExitCode::SUCCESS)
        }
    }
}

async fn list(json: bool) -> Result<ExitCode> {
    let mut device = match NetMdDevice::connect_first().await {
        Ok(device) => device,
        Err(err) => {
            eprintln!("{err}");
            eprintln!();
            udev::print_doctor_to_stderr();
            return Ok(ExitCode::FAILURE);
        }
    };

    let snapshot = device.snapshot().await?;
    if json {
        print_disc_json(&snapshot)?;
    } else {
        print_disc_human(&snapshot);
    }
    Ok(ExitCode::SUCCESS)
}
