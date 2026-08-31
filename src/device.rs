use async_trait::async_trait;
use serde::Serialize;

#[async_trait]
pub trait MinidiscDevice {
    async fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError>;
    async fn upload_raw(&mut self, request: PreparedUpload) -> Result<UploadResult, DeviceError>;
    async fn rename_disc(&mut self, title: String) -> Result<(), DeviceError>;
    async fn rename_track(&mut self, track_index: u16, title: String) -> Result<(), DeviceError>;
    async fn delete_track(&mut self, track_index: u16) -> Result<(), DeviceError>;
    async fn playback(&mut self, command: PlaybackCommand) -> Result<(), DeviceError>;
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceSnapshot {
    pub device_name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub recording_parameters: Option<Vec<u8>>,
    pub disc: Disc,
}

#[derive(Debug, Clone, Serialize)]
pub struct Disc {
    pub title: String,
    pub writable: bool,
    pub write_protected: bool,
    pub used_seconds: Option<u64>,
    pub left_seconds: Option<u64>,
    pub total_seconds: Option<u64>,
    pub track_count: usize,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub index: i32,
    pub title: Option<String>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Track {
    pub index: u16,
    pub title: Option<String>,
    pub duration_seconds: Option<u64>,
    pub channels: Option<u8>,
    pub codec: Option<String>,
    pub protected: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub title: String,
    pub format: RawUploadFormat,
    pub data: Vec<u8>,
}

impl UploadRequest {
    pub fn prepare(mut self) -> Result<PreparedUpload, UploadDataError> {
        if self.data.is_empty() {
            return Err(UploadDataError::Empty);
        }

        let frame_size = self.format.frame_size();
        let remainder = self.data.len() % frame_size;
        let padded_bytes = if remainder == 0 {
            0
        } else {
            frame_size - remainder
        };

        self.data.resize(self.data.len() + padded_bytes, 0);
        let frame_count = (self.data.len() / frame_size) as u64;

        Ok(PreparedUpload {
            title: self.title,
            format: self.format,
            data: self.data,
            padded_bytes,
            frame_count,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PreparedUpload {
    pub title: String,
    pub format: RawUploadFormat,
    pub data: Vec<u8>,
    pub padded_bytes: usize,
    pub frame_count: u64,
}

impl PreparedUpload {
    pub fn estimated_duration_seconds(&self) -> u64 {
        audio_frames_to_seconds(self.frame_count)
    }

    pub fn required_md_time_frames(&self) -> u64 {
        audio_frames_to_md_time_frames(self.frame_count)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawUploadFormat {
    Sp,
    Lp2,
    Lp105,
    Lp4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
}

impl RawUploadFormat {
    pub fn frame_size(self) -> usize {
        match self {
            RawUploadFormat::Sp => 2048,
            RawUploadFormat::Lp2 => 192,
            RawUploadFormat::Lp105 => 152,
            RawUploadFormat::Lp4 => 96,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RawUploadFormat::Sp => "SP",
            RawUploadFormat::Lp2 => "LP2",
            RawUploadFormat::Lp105 => "LP105",
            RawUploadFormat::Lp4 => "LP4",
        }
    }
}

pub fn audio_frames_to_seconds(frames: u64) -> u64 {
    (frames * 512).div_ceil(44_100)
}

pub fn audio_frames_to_md_time_frames(frames: u64) -> u64 {
    (frames * 512 * 512).div_ceil(44_100)
}

pub fn md_time_frames_to_seconds(frames: u64) -> u64 {
    frames.div_ceil(512)
}

#[derive(Debug, thiserror::Error)]
pub enum UploadDataError {
    #[error("upload audio is empty")]
    Empty,
}

#[derive(Debug, Clone)]
pub struct UploadResult {
    pub track_index: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("no supported NetMD device was found on USB")]
    NotFound,

    #[error("multiple supported NetMD devices were found; unplug all but one device for now")]
    MultipleDevices,

    #[error("could not open NetMD device: {0}")]
    Open(String),

    #[error("could not read disc contents: {0}")]
    ListContent(String),

    #[error("the inserted disc is not writable")]
    NotWritable,

    #[error("the inserted disc is write-protected")]
    WriteProtected,

    #[error("not enough space on disc: need about {needed_seconds}s, but only about {left_seconds}s remain")]
    InsufficientCapacity {
        needed_seconds: u64,
        left_seconds: u64,
    },

    #[error("could not upload track: {0}")]
    Upload(String),

    #[error("could not rename disc: {0}")]
    RenameDisc(String),

    #[error("track number must be at least 1")]
    TrackNumberZero,

    #[error("track {track_number} does not exist; disc has {track_count} tracks")]
    TrackNumberOutOfRange { track_number: u16, track_count: u16 },

    #[error("could not rename track: {0}")]
    RenameTrack(String),

    #[error("could not delete track: {0}")]
    DeleteTrack(String),

    #[error("could not control playback: {0}")]
    Playback(String),
}

#[cfg(test)]
mod tests {
    use super::{
        audio_frames_to_md_time_frames, audio_frames_to_seconds, md_time_frames_to_seconds,
        RawUploadFormat, UploadDataError, UploadRequest,
    };

    #[test]
    fn pads_upload_data_to_frame_size() {
        let prepared = UploadRequest {
            title: "tone".to_string(),
            format: RawUploadFormat::Sp,
            data: vec![1; 2050],
        }
        .prepare()
        .unwrap();

        assert_eq!(prepared.data.len(), 4096);
        assert_eq!(prepared.padded_bytes, 2046);
        assert_eq!(prepared.frame_count, 2);
    }

    #[test]
    fn rejects_empty_upload_data() {
        let error = UploadRequest {
            title: "tone".to_string(),
            format: RawUploadFormat::Lp2,
            data: Vec::new(),
        }
        .prepare()
        .unwrap_err();

        assert!(matches!(error, UploadDataError::Empty));
    }

    #[test]
    fn lp105_uses_152_byte_frames() {
        let prepared = UploadRequest {
            title: "tone".to_string(),
            format: RawUploadFormat::Lp105,
            data: vec![1; 153],
        }
        .prepare()
        .unwrap();

        assert_eq!(prepared.data.len(), 304);
        assert_eq!(prepared.padded_bytes, 151);
        assert_eq!(prepared.frame_count, 2);
    }

    #[test]
    fn upload_audio_frames_convert_to_md_capacity_frames() {
        assert_eq!(audio_frames_to_seconds(13_702), 160);
        assert_eq!(audio_frames_to_md_time_frames(13_702), 81_449);
        assert_eq!(md_time_frames_to_seconds(81_449), 160);
    }
}
