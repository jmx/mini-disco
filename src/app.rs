use crate::audio;
use crate::cli::{Cli, Command, RawFormat};
use crate::device::{MinidiscDevice, RawUploadFormat, UploadRequest};
use crate::netmd::NetMdDevice;
use crate::output::{print_disc_human, print_disc_json};
use crate::udev;
use anyhow::{Context, Result};
use std::process::ExitCode;

pub async fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::List { json } => list(json).await,
        Command::Upload {
            path,
            format,
            title,
        } => upload(path, format, title).await,
        Command::UploadRaw {
            path,
            format,
            title,
        } => upload_raw(path, format, title).await,
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

async fn upload(
    path: std::path::PathBuf,
    format: RawFormat,
    title: Option<String>,
) -> Result<ExitCode> {
    let title = title.unwrap_or_else(|| fallback_title(&path));
    let request = match audio::prepare_upload(&path, raw_upload_format(format), title) {
        Ok(request) => request,
        Err(err) => {
            eprintln!("{err}");
            return Ok(ExitCode::FAILURE);
        }
    };

    upload_request(request).await
}

async fn upload_raw(
    path: std::path::PathBuf,
    format: RawFormat,
    title: Option<String>,
) -> Result<ExitCode> {
    let data = std::fs::read(&path)
        .with_context(|| format!("could not read raw audio file `{}`", path.display()))?;
    let title = title.unwrap_or_else(|| fallback_title(&path));
    let request = UploadRequest {
        title,
        format: raw_upload_format(format),
        data,
    };

    upload_request(request).await
}

async fn upload_request(request: UploadRequest) -> Result<ExitCode> {
    let mut device = match NetMdDevice::connect_first().await {
        Ok(device) => device,
        Err(err) => {
            eprintln!("{err}");
            eprintln!();
            udev::print_doctor_to_stderr();
            return Ok(ExitCode::FAILURE);
        }
    };

    match device.upload_raw(request).await {
        Ok(result) => {
            println!("Uploaded track {}", result.track_index + 1);
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn raw_upload_format(format: RawFormat) -> RawUploadFormat {
    match format {
        RawFormat::Sp => RawUploadFormat::Sp,
        RawFormat::Lp2 => RawUploadFormat::Lp2,
        RawFormat::Lp4 => RawUploadFormat::Lp4,
    }
}

fn fallback_title(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("Untitled Track")
        .to_string()
}
