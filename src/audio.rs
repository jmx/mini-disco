use crate::device::{RawUploadFormat, UploadRequest};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::Builder;

pub fn prepare_upload(
    path: &Path,
    format: RawUploadFormat,
    title: String,
) -> Result<UploadRequest, AudioError> {
    Ok(UploadRequest {
        title,
        format,
        data: convert_to_raw(path, format)?,
    })
}

pub fn convert_to_raw(path: &Path, format: RawUploadFormat) -> Result<Vec<u8>, AudioError> {
    ensure_input_file(path)?;

    match format {
        RawUploadFormat::Sp => convert_to_sp_raw(path),
        RawUploadFormat::Lp2 | RawUploadFormat::Lp105 | RawUploadFormat::Lp4 => {
            convert_to_atrac3_raw(path, format)
        }
    }
}

fn convert_to_atrac3_raw(path: &Path, format: RawUploadFormat) -> Result<Vec<u8>, AudioError> {
    let temp_dir = Builder::new()
        .prefix("mini-disco-upload-")
        .tempdir()
        .map_err(AudioError::CreateTempDir)?;
    let wav_path = temp_dir.path().join("input.wav");
    let oma_path = temp_dir.path().join("output.oma");

    run_ffmpeg_to_wav(path, &wav_path)?;
    run_atracdenc(&wav_path, &oma_path, atracdenc_bitrate(format))?;

    let oma = fs::read(&oma_path).map_err(|err| AudioError::ReadAtracOutput {
        path: oma_path.display().to_string(),
        source: err,
    })?;
    strip_oma_header(oma)
}

fn run_ffmpeg_to_wav(input: &Path, output: &Path) -> Result<(), AudioError> {
    let output = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
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
        .output()
        .map_err(AudioError::StartFfmpeg)?;

    if !output.status.success() {
        return Err(AudioError::FfmpegFailed(command_error_detail(
            output.status,
            &output.stderr,
        )));
    }

    Ok(())
}

fn run_atracdenc(input: &Path, output: &Path, bitrate: &'static str) -> Result<(), AudioError> {
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

fn convert_to_sp_raw(path: &Path) -> Result<Vec<u8>, AudioError> {
    let output = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
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
        .output()
        .map_err(AudioError::StartFfmpeg)?;

    if !output.status.success() {
        return Err(AudioError::FfmpegFailed(command_error_detail(
            output.status,
            &output.stderr,
        )));
    }

    if output.stdout.is_empty() {
        return Err(AudioError::EmptyOutput);
    }

    Ok(output.stdout)
}

fn command_error_detail(status: std::process::ExitStatus, stderr: &[u8]) -> String {
    let detail = String::from_utf8_lossy(stderr).trim().to_string();
    if detail.is_empty() {
        status.to_string()
    } else {
        detail
    }
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
    use super::strip_oma_header;

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
}
