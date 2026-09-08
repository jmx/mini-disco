use crate::device::{RawUploadFormat, UploadRequest};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use tempfile::Builder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodingProgress {
    pub encoder: &'static str,
    pub percent: Option<u8>,
}

pub fn prepare_upload_with_progress<F>(
    path: &Path,
    format: RawUploadFormat,
    title: String,
    progress: &mut F,
) -> Result<UploadRequest, AudioError>
where
    F: FnMut(EncodingProgress),
{
    Ok(UploadRequest {
        title,
        format,
        data: convert_to_raw_with_progress(path, format, progress)?,
    })
}

pub fn convert_to_raw_with_progress<F>(
    path: &Path,
    format: RawUploadFormat,
    progress: &mut F,
) -> Result<Vec<u8>, AudioError>
where
    F: FnMut(EncodingProgress),
{
    ensure_input_file(path)?;

    match format {
        RawUploadFormat::Sp => convert_to_sp_raw(path, progress),
        RawUploadFormat::Lp2 | RawUploadFormat::Lp105 | RawUploadFormat::Lp4 => {
            convert_to_atrac3_raw(path, format, progress)
        }
    }
}

fn convert_to_atrac3_raw<F>(
    path: &Path,
    format: RawUploadFormat,
    progress: &mut F,
) -> Result<Vec<u8>, AudioError>
where
    F: FnMut(EncodingProgress),
{
    let temp_dir = Builder::new()
        .prefix("mini-disco-upload-")
        .tempdir()
        .map_err(AudioError::CreateTempDir)?;
    let wav_path = temp_dir.path().join("input.wav");
    let oma_path = temp_dir.path().join("output.oma");

    run_ffmpeg_to_wav(path, &wav_path, progress)?;
    run_atracdenc(&wav_path, &oma_path, atracdenc_bitrate(format), progress)?;

    let oma = fs::read(&oma_path).map_err(|err| AudioError::ReadAtracOutput {
        path: oma_path.display().to_string(),
        source: err,
    })?;
    strip_oma_header(oma)
}

fn run_ffmpeg_to_wav<F>(input: &Path, output: &Path, progress: &mut F) -> Result<(), AudioError>
where
    F: FnMut(EncodingProgress),
{
    let duration_us = probe_duration_us(input);
    let mut child = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-progress")
        .arg("pipe:2")
        .arg("-i")
        .arg(input)
        .arg("-vn")
        .arg("-ac")
        .arg("2")
        .arg("-ar")
        .arg("44100")
        .arg("-acodec")
        .arg("pcm_s16le")
        .arg("-f")
        .arg("wav")
        .arg(output)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(AudioError::StartFfmpeg)?;

    report_progress(progress, "ffmpeg", duration_us.map(|_| 0));
    let stderr = child
        .stderr
        .take()
        .expect("stderr is piped before spawning ffmpeg");
    let stderr_lines = read_ffmpeg_progress(stderr, duration_us, progress)?;
    let status = child.wait().map_err(AudioError::StartFfmpeg)?;

    if !status.success() {
        return Err(AudioError::FfmpegFailed(command_error_detail_from_lines(
            status,
            &stderr_lines,
        )));
    }

    report_progress(progress, "ffmpeg", Some(100));
    Ok(())
}

fn run_atracdenc<F>(
    input: &Path,
    output: &Path,
    bitrate: &'static str,
    progress: &mut F,
) -> Result<(), AudioError>
where
    F: FnMut(EncodingProgress),
{
    report_progress(progress, "atracdenc", None);
    let output = Command::new("atracdenc")
        .arg("-e")
        .arg("atrac3")
        .arg("-i")
        .arg(input)
        .arg("-o")
        .arg(output)
        .arg("--bitrate")
        .arg(bitrate)
        .output()
        .map_err(AudioError::StartAtracdenc)?;

    if !output.status.success() {
        return Err(AudioError::AtracdencFailed(command_error_detail(
            output.status,
            &output.stderr,
        )));
    }

    report_progress(progress, "atracdenc", Some(100));
    Ok(())
}

fn atracdenc_bitrate(format: RawUploadFormat) -> &'static str {
    match format {
        RawUploadFormat::Sp => unreachable!("SP conversion is rejected before atracdenc"),
        RawUploadFormat::Lp2 => "128",
        RawUploadFormat::Lp105 => "102",
        RawUploadFormat::Lp4 => "64",
    }
}

fn strip_oma_header(oma: Vec<u8>) -> Result<Vec<u8>, AudioError> {
    const OMA_HEADER_BYTES: usize = 96;
    if oma.len() <= OMA_HEADER_BYTES {
        return Err(AudioError::AtracOutputTooShort(oma.len()));
    }

    Ok(oma[OMA_HEADER_BYTES..].to_vec())
}

fn ensure_input_file(path: &Path) -> Result<(), AudioError> {
    match path.try_exists() {
        Ok(true) if path.is_file() => Ok(()),
        Ok(true) => Err(AudioError::NotAFile(path.display().to_string())),
        Ok(false) => Err(AudioError::InputNotFound(path.display().to_string())),
        Err(err) => Err(AudioError::InspectInput(err)),
    }
}

fn convert_to_sp_raw<F>(path: &Path, progress: &mut F) -> Result<Vec<u8>, AudioError>
where
    F: FnMut(EncodingProgress),
{
    let duration_us = probe_duration_us(path);
    let mut child = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-progress")
        .arg("pipe:2")
        .arg("-i")
        .arg(path)
        .arg("-vn")
        .arg("-ac")
        .arg("2")
        .arg("-ar")
        .arg("44100")
        .arg("-acodec")
        .arg("pcm_s16be")
        .arg("-f")
        .arg("s16be")
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(AudioError::StartFfmpeg)?;

    let mut stdout = child
        .stdout
        .take()
        .expect("stdout is piped before spawning ffmpeg");
    let stdout_reader = thread::spawn(move || {
        let mut data = Vec::new();
        stdout.read_to_end(&mut data).map(|_| data)
    });

    report_progress(progress, "ffmpeg", duration_us.map(|_| 0));
    let stderr = child
        .stderr
        .take()
        .expect("stderr is piped before spawning ffmpeg");
    let stderr_lines = read_ffmpeg_progress(stderr, duration_us, progress)?;
    let status = child.wait().map_err(AudioError::StartFfmpeg)?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| AudioError::FfmpegFailed("could not read ffmpeg stdout".to_string()))?
        .map_err(AudioError::ReadFfmpegOutput)?;

    if !status.success() {
        return Err(AudioError::FfmpegFailed(command_error_detail_from_lines(
            status,
            &stderr_lines,
        )));
    }

    if stdout.is_empty() {
        return Err(AudioError::EmptyOutput);
    }

    report_progress(progress, "ffmpeg", Some(100));
    Ok(stdout)
}

fn command_error_detail(status: std::process::ExitStatus, stderr: &[u8]) -> String {
    let detail = String::from_utf8_lossy(stderr).trim().to_string();
    if detail.is_empty() {
        status.to_string()
    } else {
        detail
    }
}

fn command_error_detail_from_lines(status: ExitStatus, stderr_lines: &[String]) -> String {
    let detail = stderr_lines.join("\n").trim().to_string();
    if detail.is_empty() {
        status.to_string()
    } else {
        detail
    }
}

fn probe_duration_us(path: &Path) -> Option<u64> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let seconds: f64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;
    if seconds.is_finite() && seconds > 0.0 {
        Some((seconds * 1_000_000.0).round() as u64)
    } else {
        None
    }
}

fn read_ffmpeg_progress<F, R>(
    stderr: R,
    duration_us: Option<u64>,
    progress: &mut F,
) -> Result<Vec<String>, AudioError>
where
    F: FnMut(EncodingProgress),
    R: Read,
{
    let mut stderr_lines = Vec::new();
    let mut last_percent = duration_us.map(|_| 0);

    for line in BufReader::new(stderr).lines() {
        let line = line.map_err(AudioError::ReadFfmpegOutput)?;
        if is_ffmpeg_progress_line(&line) {
            if let Some(percent) = parse_ffmpeg_progress_percent(&line, duration_us) {
                if Some(percent) != last_percent {
                    last_percent = Some(percent);
                    report_progress(progress, "ffmpeg", Some(percent));
                }
            }
        } else if !line.trim().is_empty() {
            stderr_lines.push(line);
        }
    }

    Ok(stderr_lines)
}

fn parse_ffmpeg_progress_percent(line: &str, duration_us: Option<u64>) -> Option<u8> {
    let duration_us = duration_us?;
    let current_us = line
        .strip_prefix("out_time_us=")
        .or_else(|| line.strip_prefix("out_time_ms="))
        .and_then(|value| value.trim().parse::<u64>().ok())
        .or_else(|| {
            line.strip_prefix("out_time=")
                .and_then(|value| parse_ffmpeg_timestamp_us(value.trim()))
        })?;

    Some(percent_from_duration(current_us, duration_us))
}

fn parse_ffmpeg_timestamp_us(value: &str) -> Option<u64> {
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<u64>().ok()?;
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let seconds = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let mut second_parts = seconds.split('.');
    let whole_seconds = second_parts.next()?.parse::<u64>().ok()?;
    let fractional = second_parts.next().unwrap_or("");
    if second_parts.next().is_some() {
        return None;
    }

    let micros = if fractional.is_empty() {
        0
    } else {
        let mut padded = fractional.chars().take(6).collect::<String>();
        while padded.len() < 6 {
            padded.push('0');
        }
        padded.parse::<u64>().ok()?
    };

    Some(((hours * 60 + minutes) * 60 + whole_seconds) * 1_000_000 + micros)
}

fn percent_from_duration(current_us: u64, duration_us: u64) -> u8 {
    if duration_us == 0 {
        return 0;
    }
    ((current_us.saturating_mul(100) / duration_us).min(99)) as u8
}

fn is_ffmpeg_progress_line(line: &str) -> bool {
    let Some((key, _value)) = line.split_once('=') else {
        return false;
    };

    if key.starts_with("stream_") && key.ends_with("_q") {
        return true;
    }

    matches!(
        key,
        "bitrate"
            | "drop_frames"
            | "dup_frames"
            | "fps"
            | "frame"
            | "out_time"
            | "out_time_ms"
            | "out_time_us"
            | "progress"
            | "speed"
            | "total_size"
    )
}

fn report_progress<F>(progress: &mut F, encoder: &'static str, percent: Option<u8>)
where
    F: FnMut(EncodingProgress),
{
    progress(EncodingProgress { encoder, percent });
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("input audio file does not exist: {0}")]
    InputNotFound(String),

    #[error("input audio path is not a file: {0}")]
    NotAFile(String),

    #[error("could not inspect input audio file: {0}")]
    InspectInput(std::io::Error),

    #[error("could not start ffmpeg: {0}")]
    StartFfmpeg(std::io::Error),

    #[error("ffmpeg failed: {0}")]
    FfmpegFailed(String),

    #[error("could not read ffmpeg output: {0}")]
    ReadFfmpegOutput(std::io::Error),

    #[error("could not create temporary conversion directory: {0}")]
    CreateTempDir(std::io::Error),

    #[error("could not start atracdenc: {0}. Install atracdenc or use upload-raw with prepared ATRAC3 frames")]
    StartAtracdenc(std::io::Error),

    #[error("atracdenc failed: {0}")]
    AtracdencFailed(String),

    #[error("could not read ATRAC output `{path}`: {source}")]
    ReadAtracOutput {
        path: String,
        source: std::io::Error,
    },

    #[error("atracdenc output is too short to contain an OMA header and ATRAC data: {0} bytes")]
    AtracOutputTooShort(usize),

    #[error("ffmpeg produced no audio data")]
    EmptyOutput,
}

#[cfg(test)]
mod tests {
    use super::{
        is_ffmpeg_progress_line, parse_ffmpeg_progress_percent, parse_ffmpeg_timestamp_us,
        percent_from_duration, strip_oma_header,
    };

    #[test]
    fn strips_oma_header() {
        let mut data = vec![0; 96];
        data.extend_from_slice(&[1, 2, 3]);

        assert_eq!(strip_oma_header(data).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn rejects_short_oma_output() {
        assert!(strip_oma_header(vec![0; 96]).is_err());
    }

    #[test]
    fn parses_ffmpeg_timestamp_as_microseconds() {
        assert_eq!(
            parse_ffmpeg_timestamp_us("01:02:03.4567"),
            Some(3_723_456_700)
        );
    }

    #[test]
    fn caps_progress_below_complete_until_process_succeeds() {
        assert_eq!(percent_from_duration(50, 100), 50);
        assert_eq!(percent_from_duration(100, 100), 99);
        assert_eq!(percent_from_duration(150, 100), 99);
    }

    #[test]
    fn parses_ffmpeg_out_time_progress_line() {
        assert_eq!(
            parse_ffmpeg_progress_percent("out_time_us=2500000", Some(10_000_000)),
            Some(25)
        );
        assert_eq!(
            parse_ffmpeg_progress_percent("out_time=00:00:05.000000", Some(10_000_000)),
            Some(50)
        );
    }

    #[test]
    fn recognizes_ffmpeg_stream_quality_progress_lines() {
        assert!(is_ffmpeg_progress_line("stream_0_1_q=-0.0"));
    }
}
