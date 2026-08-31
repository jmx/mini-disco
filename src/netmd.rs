use crate::device::{
    md_time_frames_to_seconds, DeviceError, DeviceSnapshot, Disc, Group, MinidiscDevice,
    PreparedUpload, RawUploadFormat, Track, UploadResult,
};
use async_trait::async_trait;
use cross_usb::get_device_list;
use minidisc::netmd::base::DEVICE_IDS_CROSSUSB;
use minidisc::netmd::interface::{DiscFlag, MDTrack, WireFormat};
use minidisc::netmd::NetMDContext;

pub struct NetMdDevice {
    context: NetMDContext,
}

impl NetMdDevice {
    pub async fn connect_first() -> Result<Self, DeviceError> {
        let descriptors: Vec<_> = get_device_list(DEVICE_IDS_CROSSUSB.to_vec())
            .await
            .map_err(|err| map_open_error(err.to_string()))?
            .collect();

        let descriptor = match descriptors.len() {
            0 => return Err(DeviceError::NotFound),
            1 => descriptors.into_iter().next().expect("length checked"),
            _ => return Err(DeviceError::MultipleDevices),
        };

        let context = NetMDContext::new(descriptor)
            .await
            .map_err(|err| DeviceError::Open(err.to_string()))?;

        Ok(Self { context })
    }
}

#[async_trait]
impl MinidiscDevice for NetMdDevice {
    async fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
        let device_name = self
            .context
            .interface()
            .device
            .device_name()
            .unwrap_or("Unknown NetMD device")
            .to_string();
        let vendor_id = self.context.interface().device.vendor_id();
        let product_id = self.context.interface().device.product_id();
        let recording_parameters = self
            .context
            .interface_mut()
            .recording_parameters()
            .await
            .ok();
        let disc = read_disc(&mut self.context).await?;

        Ok(DeviceSnapshot {
            device_name,
            vendor_id,
            product_id,
            recording_parameters,
            disc,
        })
    }

    async fn upload_raw(&mut self, request: PreparedUpload) -> Result<UploadResult, DeviceError> {
        ensure_upload_allowed(&mut self.context, request.required_md_time_frames()).await?;

        let track = MDTrack {
            title: request.title,
            format: wire_format(request.format),
            data: request.data,
            chunk_size: 0x400,
            full_width_title: None,
        };

        let (track_index, _uuid, _ccid) = self
            .context
            .download(track, |total: usize, written: usize| {
                eprint!("\rUploading: {written}/{total} bytes");
                let _ = std::io::Write::flush(&mut std::io::stderr());
            })
            .await
            .map_err(|err| DeviceError::Upload(err.to_string()))?;

        eprintln!();
        Ok(UploadResult { track_index })
    }
}

fn map_open_error(message: String) -> DeviceError {
    let lower = message.to_lowercase();
    if lower.contains("not found") || lower.contains("no device") {
        DeviceError::NotFound
    } else {
        DeviceError::Open(message)
    }
}

async fn read_disc(context: &mut NetMDContext) -> Result<Disc, DeviceError> {
    let interface = context.interface_mut();
    let flags = interface
        .disc_flags()
        .await
        .map_err(|err| DeviceError::ListContent(err.to_string()))?;
    let title = interface
        .disc_title(false)
        .await
        .map_err(|err| DeviceError::ListContent(err.to_string()))?;
    let capacity = interface
        .disc_capacity()
        .await
        .map_err(|err| DeviceError::ListContent(err.to_string()))?;
    let track_count = interface
        .track_count()
        .await
        .map_err(|err| DeviceError::ListContent(err.to_string()))?;
    let track_groups = interface
        .track_group_list()
        .await
        .map_err(|err| DeviceError::ListContent(err.to_string()))?;

    let mut groups = Vec::new();
    for (index, (title, _full_width_title, track_indexes)) in track_groups.into_iter().enumerate() {
        let mut tracks = Vec::new();
        for track_index in track_indexes {
            let title = interface
                .track_title(track_index, false)
                .await
                .map_err(|err| DeviceError::ListContent(err.to_string()))?;
            let duration = interface
                .track_length(track_index)
                .await
                .map_err(|err| DeviceError::ListContent(err.to_string()))?;
            let (encoding, channels) = interface
                .track_encoding(track_index)
                .await
                .map_err(|err| DeviceError::ListContent(err.to_string()))?;
            let flags = interface
                .track_flags(track_index)
                .await
                .map_err(|err| DeviceError::ListContent(err.to_string()))?;

            tracks.push(Track {
                index: track_index,
                title: blank_to_none(title),
                duration_seconds: Some(md_time_frames_to_seconds(duration.as_frames())),
                channels: Some(match channels {
                    minidisc::netmd::interface::Channels::Mono => 1,
                    minidisc::netmd::interface::Channels::Stereo => 2,
                }),
                codec: Some(encoding.to_string()),
                protected: Some(match flags {
                    0x03 => "protected".to_string(),
                    0x00 => "unprotected".to_string(),
                    other => format!("unknown(0x{other:02x})"),
                }),
            });
        }

        groups.push(Group {
            index: index as i32,
            title: title.and_then(blank_to_none),
            tracks,
        });
    }

    let mut capacity_frames = [
        capacity[0].as_frames(),
        capacity[1].as_frames(),
        capacity[2].as_frames(),
    ];
    while capacity_frames[1] > 512 * 60 * 82 {
        capacity_frames[0] /= 2;
        capacity_frames[1] /= 2;
        capacity_frames[2] /= 2;
    }

    Ok(Disc {
        title,
        writable: (flags & DiscFlag::Writable as u8) != 0,
        write_protected: (flags & DiscFlag::WriteProtected as u8) != 0,
        used_seconds: Some(md_time_frames_to_seconds(capacity_frames[0])),
        left_seconds: Some(md_time_frames_to_seconds(capacity_frames[2])),
        total_seconds: Some(md_time_frames_to_seconds(capacity_frames[1])),
        track_count: track_count as usize,
        groups,
    })
}

async fn ensure_upload_allowed(
    context: &mut NetMDContext,
    md_time_frames_needed: u64,
) -> Result<(), DeviceError> {
    let interface = context.interface_mut();
    let flags = interface
        .disc_flags()
        .await
        .map_err(|err| DeviceError::ListContent(err.to_string()))?;

    if (flags & DiscFlag::WriteProtected as u8) != 0 {
        return Err(DeviceError::WriteProtected);
    }

    if (flags & DiscFlag::Writable as u8) == 0 {
        return Err(DeviceError::NotWritable);
    }

    let capacity = interface
        .disc_capacity()
        .await
        .map_err(|err| DeviceError::ListContent(err.to_string()))?;
    let mut total_frames = capacity[1].as_frames();
    let mut left_frames = capacity[2].as_frames();
    while total_frames > 512 * 60 * 82 {
        total_frames /= 2;
        left_frames /= 2;
    }

    if md_time_frames_needed > left_frames {
        return Err(DeviceError::InsufficientCapacity {
            needed_seconds: md_time_frames_to_seconds(md_time_frames_needed),
            left_seconds: md_time_frames_to_seconds(left_frames),
        });
    }

    Ok(())
}

fn wire_format(format: RawUploadFormat) -> WireFormat {
    match format {
        RawUploadFormat::Sp => WireFormat::Pcm,
        RawUploadFormat::Lp2 => WireFormat::LP2,
        RawUploadFormat::Lp105 => WireFormat::L105kbps,
        RawUploadFormat::Lp4 => WireFormat::LP4,
    }
}

fn blank_to_none(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}
