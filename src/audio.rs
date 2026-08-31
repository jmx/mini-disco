use crate::device::{RawUploadFormat, UploadRequest};
use std::path::Path;
use std::process::Command;

pub fn prepare_upload(
    path: &Path,
    format: RawUploadFormat,
    title: String,
) -> Result<UploadRequest, AudioError> {
    match format {
        RawUploadFormat::Sp => Ok(UploadRequest {
            title,
            format,
            data: convert_to_sp_raw(path)?,
        }),
        RawUploadFormat::Lp2 | RawUploadFormat::Lp4 => Err(AudioError::UnsupportedFormat(format)),
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
        return Err(AudioError::FfmpegFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    if output.stdout.is_empty() {
        return Err(AudioError::EmptyOutput);
    }

    Ok(output.stdout)
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("could not start ffmpeg: {0}")]
    StartFfmpeg(std::io::Error),

    #[error("ffmpeg failed: {0}")]
    FfmpegFailed(String),

    #[error("ffmpeg produced no audio data")]
    EmptyOutput,

    #[error("ffmpeg conversion for {0:?} is not implemented yet; use upload-raw with prepared ATRAC3 frames")]
    UnsupportedFormat(RawUploadFormat),
}
