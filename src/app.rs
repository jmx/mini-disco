use crate::audio;
use crate::cli::{Cli, Command, RawFormat};
use crate::device::{MinidiscDevice, PreparedUpload, RawUploadFormat, UploadRequest};
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
        Command::Convert {
            input,
            output,
            format,
        } => convert(input, output, format),
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

fn convert(
    input: std::path::PathBuf,
    output: std::path::PathBuf,
    format: RawFormat,
) -> Result<ExitCode> {
    let data = match audio::convert_to_raw(&input, raw_upload_format(format)) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("{err}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let request = UploadRequest {
        title: fallback_title(&input),
        format: raw_upload_format(format),
        data,
    };

    let prepared = match request.prepare() {
        Ok(prepared) => prepared,
        Err(err) => {
            eprintln!("{err}");
            return Ok(ExitCode::FAILURE);
        }
    };

    std::fs::write(&output, &prepared.data)
        .with_context(|| format!("could not write raw output `{}`", output.display()))?;
    print_upload_summary(&prepared);
    println!("Wrote {}", output.display());

    Ok(ExitCode::SUCCESS)
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
    let request = match request.prepare() {
        Ok(request) => request,
        Err(err) => {
            eprintln!("{err}");
            return Ok(ExitCode::FAILURE);
        }
    };

    print_upload_summary(&request);
    let upload_format = request.format;

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
            println!();
            match device.snapshot().await {
                Ok(snapshot) => print_disc_human(&snapshot),
                Err(err) => eprintln!("Uploaded, but could not refresh disc contents: {err}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("{err}");
            print_upload_failure_hint(upload_format, &err);
            Ok(ExitCode::FAILURE)
        }
    }
}

fn print_upload_failure_hint(format: RawUploadFormat, err: &crate::device::DeviceError) {
    let message = err.to_string();
    if format == RawUploadFormat::Lp2
        && message.contains("send track failed")
        && message.contains("the device rejected the message")
        && message.contains(" 94, 02, ")
    {
        eprintln!();
        eprintln!(
            "Hint: this deck rejected the normal LP2 reservation. Retry with `--format lp105`."
        );
    }
}

fn print_upload_summary(request: &PreparedUpload) {
    eprintln!(
        "Prepared {} upload: about {}s, {} bytes",
        request.format.label(),
        request.estimated_duration_seconds(),
        request.data.len()
    );
    if request.padded_bytes > 0 {
        eprintln!(
            "Padded final NetMD frame with {} bytes of silence",
            request.padded_bytes
        );
    }
}

fn raw_upload_format(format: RawFormat) -> RawUploadFormat {
    match format {
        RawFormat::Sp => RawUploadFormat::Sp,
        RawFormat::Lp2 => RawUploadFormat::Lp2,
        RawFormat::Lp105 => RawUploadFormat::Lp105,
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
