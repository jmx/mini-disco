use async_trait::async_trait;
use serde::Serialize;

#[async_trait]
pub trait MinidiscDevice {
    async fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError>;
    async fn upload_raw(&mut self, request: UploadRequest) -> Result<UploadResult, DeviceError>;
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceSnapshot {
    pub device_name: String,
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

#[derive(Clone, Copy, Debug)]
pub enum RawUploadFormat {
    Sp,
    Lp2,
    Lp4,
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

    #[error("could not upload track: {0}")]
    Upload(String),
}
