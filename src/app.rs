use crate::audio;
use crate::cli::{Cli, Command, RawFormat};
use crate::device::{
    DeviceError, MinidiscDevice, PlaybackCommand, PreparedUpload, RawUploadFormat, UploadRequest,
};
use crate::m3u;
use crate::netmd::NetMdDevice;
use crate::output::{print_devices_human, print_devices_json, print_disc_human, print_disc_json};
use crate::udev;
use anyhow::{Context, Result};
use std::io::Write;
use std::process::ExitCode;

pub async fn run(cli: Cli) -> Result<ExitCode> {
    let device_index = cli.device;
    match cli.command {
        Command::Devices { json } => devices(json).await,
        Command::List { json } => list(device_index, json).await,
        Command::Upload {
            path,
            format,
            title,
        } => upload(device_index, path, format, title).await,
        Command::UploadM3u {
            path,
            format,
            erase_first,
            group,
        } => upload_m3u(device_index, path, format, erase_first, group).await,
        Command::UploadRaw {
            path,
            format,
            title,
        } => upload_raw(device_index, path, format, title).await,
        Command::Convert {
            input,
            output,
            format,
        } => convert(input, output, format),
        Command::RenameDisc { title } => rename_disc(device_index, title).await,
        Command::RenameTrack { track, title } => rename_track(device_index, track, title).await,
        Command::DeleteTrack { track } => delete_track(device_index, track).await,
        Command::Erase => erase_disc(device_index).await,
        Command::Play => playback(device_index, PlaybackCommand::Play, "Started playback").await,
        Command::Pause => playback(device_index, PlaybackCommand::Pause, "Paused playback").await,
        Command::Stop => playback(device_index, PlaybackCommand::Stop, "Stopped playback").await,
        Command::Next => {
            playback(device_index, PlaybackCommand::Next, "Skipped to next track").await
        }
        Command::Prev => {
            playback(
                device_index,
                PlaybackCommand::Previous,
                "Skipped to previous track",
            )
            .await
        }
        Command::Doctor => {
            udev::print_doctor();
            Ok(ExitCode::SUCCESS)
        }
    }
}

async fn devices(json: bool) -> Result<ExitCode> {
    let devices = NetMdDevice::list_devices().await?;
    if json {
        print_devices_json(&devices)?;
    } else {
        print_devices_human(&devices);
    }
    Ok(ExitCode::SUCCESS)
}

async fn list(device_index: Option<usize>, json: bool) -> Result<ExitCode> {
    let mut device = match connect_device(device_index).await {
        Ok(device) => device,
        Err(code) => return Ok(code),
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
    device_index: Option<usize>,
    path: std::path::PathBuf,
    format: RawFormat,
    title: Option<String>,
) -> Result<ExitCode> {
    let title = title.unwrap_or_else(|| fallback_title(&path));
    let mut progress = EncodingProgressPrinter::new(format!("Encoding `{}`", path.display()));
    let mut report = |event| progress.report(event);
    let request_result =
        audio::prepare_upload_with_progress(&path, raw_upload_format(format), title, &mut report);
    drop(report);
    progress.finish();

    let request = match request_result {
        Ok(request) => request,
        Err(err) => {
            eprintln!("{err}");
            return Ok(ExitCode::FAILURE);
        }
    };

    upload_request(device_index, request).await
}

async fn upload_m3u(
    device_index: Option<usize>,
    path: std::path::PathBuf,
    format: RawFormat,
    erase_first: bool,
    group: bool,
) -> Result<ExitCode> {
    let playlist = match m3u::read_playlist(&path) {
        Ok(playlist) => playlist,
        Err(err) => {
            eprintln!("{err}");
            return Ok(ExitCode::FAILURE);
        }
    };
    let playlist_title = playlist
        .title
        .clone()
        .unwrap_or_else(|| fallback_title(&path));

    let mut prepared_uploads = Vec::new();
    let track_total = playlist.tracks.len();
    for (index, track) in playlist.tracks.iter().enumerate() {
        let title = track
            .title
            .clone()
            .unwrap_or_else(|| fallback_title(&track.path));
        let mut progress = EncodingProgressPrinter::new(format!(
            "Encoding track {}/{} `{}`",
            index + 1,
            track_total,
            track.path.display()
        ));
        let mut report = |event| progress.report(event);
        let request_result = audio::prepare_upload_with_progress(
            &track.path,
            raw_upload_format(format),
            title,
            &mut report,
        );
        drop(report);
        progress.finish();

        let request = match request_result {
            Ok(request) => request,
            Err(err) => {
                eprintln!(
                    "Could not prepare playlist track {} (`{}`): {err}",
                    index + 1,
                    track.path.display()
                );
                return Ok(ExitCode::FAILURE);
            }
        };
        let request = match request.prepare() {
            Ok(request) => request,
            Err(err) => {
                eprintln!(
                    "Could not prepare playlist track {} (`{}`): {err}",
                    index + 1,
                    track.path.display()
                );
                return Ok(ExitCode::FAILURE);
            }
        };
        prepared_uploads.push(request);
    }

    eprintln!(
        "Prepared M3U upload: {} track{}",
        prepared_uploads.len(),
        if prepared_uploads.len() == 1 { "" } else { "s" }
    );

    let mut device = match connect_device(device_index).await {
        Ok(device) => device,
        Err(code) => return Ok(code),
    };

    if erase_first {
        match device.erase_disc().await {
            Ok(()) => println!("Erased disc"),
            Err(err) => {
                eprintln!("{err}");
                return Ok(ExitCode::FAILURE);
            }
        }
    }

    if !group {
        match device.rename_disc(playlist_title.clone()).await {
            Ok(()) => println!("Renamed disc"),
            Err(err) => {
                eprintln!("{err}");
                return Ok(ExitCode::FAILURE);
            }
        }
    }

    let track_count = match u16::try_from(prepared_uploads.len()) {
        Ok(track_count) => track_count,
        Err(_) => {
            eprintln!("M3U playlist contains too many tracks for one MiniDisc group");
            return Ok(ExitCode::FAILURE);
        }
    };

    let mut first_uploaded_track = None;
    for request in prepared_uploads {
        print_upload_summary(&request);
        let upload_format = request.format;
        match device.upload_raw(request).await {
            Ok(result) => {
                if first_uploaded_track.is_none() {
                    first_uploaded_track = Some(result.track_index);
                }
                println!("Uploaded track {}", result.track_index + 1);
            }
            Err(err) => {
                eprintln!("{err}");
                print_upload_failure_hint(upload_format, &err);
                return Ok(ExitCode::FAILURE);
            }
        }
    }

    if group {
        let Some(start_track_index) = first_uploaded_track else {
            eprintln!("M3U playlist does not contain any tracks to group");
            return Ok(ExitCode::FAILURE);
        };
        match device
            .add_group(start_track_index, track_count, playlist_title.clone())
            .await
        {
            Ok(()) => println!("Created group `{playlist_title}`"),
            Err(err) => {
                eprintln!("{err}");
                return Ok(ExitCode::FAILURE);
            }
        }
    }

    println!();
    match device.snapshot().await {
        Ok(snapshot) => print_disc_human(&snapshot),
        Err(err) => eprintln!("Uploaded, but could not refresh disc contents: {err}"),
    }

    Ok(ExitCode::SUCCESS)
}

fn convert(
    input: std::path::PathBuf,
    output: std::path::PathBuf,
    format: RawFormat,
) -> Result<ExitCode> {
    let mut progress = EncodingProgressPrinter::new(format!("Encoding `{}`", input.display()));
    let mut report = |event| progress.report(event);
    let data_result =
        audio::convert_to_raw_with_progress(&input, raw_upload_format(format), &mut report);
    drop(report);
    progress.finish();

    let data = match data_result {
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
    device_index: Option<usize>,
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

    upload_request(device_index, request).await
}

async fn rename_disc(device_index: Option<usize>, title: String) -> Result<ExitCode> {
    let mut device = match connect_device(device_index).await {
        Ok(device) => device,
        Err(code) => return Ok(code),
    };

    match device.rename_disc(title).await {
        Ok(()) => {
            println!("Renamed disc");
            println!();
            match device.snapshot().await {
                Ok(snapshot) => print_disc_human(&snapshot),
                Err(err) => eprintln!("Renamed, but could not refresh disc contents: {err}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(ExitCode::FAILURE)
        }
    }
}

async fn rename_track(
    device_index: Option<usize>,
    track_number: u16,
    title: String,
) -> Result<ExitCode> {
    let track_index = match track_number_to_index(track_number) {
        Ok(track_index) => track_index,
        Err(err) => {
            eprintln!("{err}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let mut device = match connect_device(device_index).await {
        Ok(device) => device,
        Err(code) => return Ok(code),
    };

    match device.rename_track(track_index, title).await {
        Ok(()) => {
            println!("Renamed track {track_number}");
            println!();
            match device.snapshot().await {
                Ok(snapshot) => print_disc_human(&snapshot),
                Err(err) => eprintln!("Renamed, but could not refresh disc contents: {err}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(ExitCode::FAILURE)
        }
    }
}

async fn delete_track(device_index: Option<usize>, track_number: u16) -> Result<ExitCode> {
    let track_index = match track_number_to_index(track_number) {
        Ok(track_index) => track_index,
        Err(err) => {
            eprintln!("{err}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let mut device = match connect_device(device_index).await {
        Ok(device) => device,
        Err(code) => return Ok(code),
    };

    match device.delete_track(track_index).await {
        Ok(()) => {
            println!("Deleted track {track_number}");
            println!();
            match device.snapshot().await {
                Ok(snapshot) => print_disc_human(&snapshot),
                Err(err) => eprintln!("Deleted, but could not refresh disc contents: {err}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(ExitCode::FAILURE)
        }
    }
}

async fn erase_disc(device_index: Option<usize>) -> Result<ExitCode> {
    let mut device = match connect_device(device_index).await {
        Ok(device) => device,
        Err(code) => return Ok(code),
    };

    match device.erase_disc().await {
        Ok(()) => {
            println!("Erased disc");
            println!();
            match device.snapshot().await {
                Ok(snapshot) => print_disc_human(&snapshot),
                Err(err) => eprintln!("Erased, but could not refresh disc contents: {err}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(ExitCode::FAILURE)
        }
    }
}

async fn playback(
    device_index: Option<usize>,
    command: PlaybackCommand,
    success_message: &str,
) -> Result<ExitCode> {
    let mut device = match connect_device(device_index).await {
        Ok(device) => device,
        Err(code) => return Ok(code),
    };

    match device.playback(command).await {
        Ok(()) => {
            println!("{success_message}");
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(ExitCode::FAILURE)
        }
    }
}

async fn upload_request(device_index: Option<usize>, request: UploadRequest) -> Result<ExitCode> {
    let request = match request.prepare() {
        Ok(request) => request,
        Err(err) => {
            eprintln!("{err}");
            return Ok(ExitCode::FAILURE);
        }
    };

    print_upload_summary(&request);
    let upload_format = request.format;

    let mut device = match connect_device(device_index).await {
        Ok(device) => device,
        Err(code) => return Ok(code),
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

async fn connect_device(device_index: Option<usize>) -> Result<NetMdDevice, ExitCode> {
    match NetMdDevice::connect(device_index).await {
        Ok(device) => Ok(device),
        Err(err) => {
            eprintln!("{err}");
            if matches!(err, DeviceError::NotFound | DeviceError::Open(_)) {
                eprintln!();
                udev::print_doctor_to_stderr();
            }
            Err(ExitCode::FAILURE)
        }
    }
}

struct EncodingProgressPrinter {
    prefix: String,
    last_len: usize,
}

impl EncodingProgressPrinter {
    fn new(prefix: String) -> Self {
        Self {
            prefix,
            last_len: 0,
        }
    }

    fn report(&mut self, progress: audio::EncodingProgress) {
        let value = match progress.percent {
            Some(percent) => format!("{percent}%"),
            None => "running".to_string(),
        };
        let line = format!("{} with {}: {}", self.prefix, progress.encoder, value);
        let padding = self.last_len.saturating_sub(line.len());
        eprint!("\r{line}{}", " ".repeat(padding));
        let _ = std::io::stderr().flush();
        self.last_len = line.len();
    }

    fn finish(&mut self) {
        if self.last_len > 0 {
            eprintln!();
            self.last_len = 0;
        }
    }
}

fn track_number_to_index(track_number: u16) -> Result<u16, crate::device::DeviceError> {
    track_number
        .checked_sub(1)
        .ok_or(crate::device::DeviceError::TrackNumberZero)
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

#[cfg(test)]
mod tests {
    use super::track_number_to_index;
    use crate::device::DeviceError;

    #[test]
    fn converts_display_track_number_to_netmd_index() {
        assert_eq!(track_number_to_index(1).unwrap(), 0);
        assert_eq!(track_number_to_index(42).unwrap(), 41);
    }

    #[test]
    fn rejects_zero_track_number() {
        assert!(matches!(
            track_number_to_index(0).unwrap_err(),
            DeviceError::TrackNumberZero
        ));
    }
}
