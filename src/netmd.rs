use crate::device::{
    md_time_frames_to_seconds, DeviceError, DeviceSnapshot, Disc, Group, MinidiscDevice,
    PlaybackCommand, PreparedUpload, RawUploadFormat, Track, UploadResult,
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

    async fn rename_disc(&mut self, title: String) -> Result<(), DeviceError> {
        ensure_disc_writable(&mut self.context)
            .await
            .map_err(map_rename_precondition_error)?;
        self.context
            .rename_disc(&title, None)
            .await
            .map_err(|err| DeviceError::RenameDisc(err.to_string()))
    }

    async fn rename_track(&mut self, track_index: u16, title: String) -> Result<(), DeviceError> {
        ensure_track_write_allowed(&mut self.context, track_index)
            .await
            .map_err(map_rename_track_precondition_error)?;
        self.context
            .interface_mut()
            .set_track_title(track_index, &title, false)
            .await
            .map_err(|err| DeviceError::RenameTrack(err.to_string()))
    }

    async fn delete_track(&mut self, track_index: u16) -> Result<(), DeviceError> {
        ensure_track_write_allowed(&mut self.context, track_index)
            .await
            .map_err(map_delete_track_precondition_error)?;
        self.context
            .interface_mut()
            .erase_track(track_index)
            .await
            .map_err(|err| DeviceError::DeleteTrack(err.to_string()))
    }

    async fn erase_disc(&mut self) -> Result<(), DeviceError> {
        ensure_disc_writable(&mut self.context)
            .await
            .map_err(map_erase_precondition_error)?;
        self.context
            .interface_mut()
            .erase_disc()
            .await
            .map_err(|err| DeviceError::EraseDisc(err.to_string()))
    }

    async fn add_group(
        &mut self,
        start_track_index: u16,
        track_count: u16,
        title: String,
    ) -> Result<(), DeviceError> {
        ensure_disc_writable(&mut self.context)
            .await
            .map_err(map_create_group_precondition_error)?;
        if track_count == 0 {
            return Err(DeviceError::EmptyGroup);
        }

        let disc = read_disc(&mut self.context)
            .await
            .map_err(map_create_group_precondition_error)?;
        let end_track_index = start_track_index
            .checked_add(track_count - 1)
            .ok_or_else(|| {
                DeviceError::CreateGroup("track range exceeds supported track numbers".into())
            })?;

        if end_track_index as usize >= disc.track_count {
            return Err(DeviceError::TrackNumberOutOfRange {
                track_number: end_track_index + 1,
                track_count: disc.track_count as u16,
            });
        }

        let (half_width_title, full_width_title) =
            compile_group_titles(&disc, start_track_index, track_count, &title);
        let interface = self.context.interface_mut();
        interface
            .set_disc_title(&half_width_title, false)
            .await
            .or_else(ignore_unchanged_title)
            .map_err(|err| DeviceError::CreateGroup(err.to_string()))?;
        interface
            .set_disc_title(&full_width_title, true)
            .await
            .or_else(ignore_unchanged_title)
            .map_err(|err| DeviceError::CreateGroup(err.to_string()))
    }

    async fn playback(&mut self, command: PlaybackCommand) -> Result<(), DeviceError> {
        match command {
            PlaybackCommand::Play => self.context.interface_mut().play().await,
            PlaybackCommand::Pause => self.context.interface_mut().pause().await,
            PlaybackCommand::Stop => self.context.interface_mut().stop().await,
            PlaybackCommand::Next => self.context.next_track().await,
            PlaybackCommand::Previous => self.context.previous_track().await,
        }
        .map_err(|err| DeviceError::Playback(err.to_string()))
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
    let full_width_title = interface
        .disc_title(true)
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
    for (index, (title, full_width_title, track_indexes)) in track_groups.into_iter().enumerate() {
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
            full_width_title: full_width_title.and_then(blank_to_none),
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
        full_width_title,
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
    ensure_disc_writable(context).await?;

    let capacity = context
        .interface_mut()
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

async fn ensure_disc_writable(context: &mut NetMDContext) -> Result<(), DeviceError> {
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

    Ok(())
}

fn map_rename_precondition_error(err: DeviceError) -> DeviceError {
    match err {
        DeviceError::ListContent(message) => DeviceError::RenameDisc(message),
        other => other,
    }
}

async fn ensure_track_write_allowed(
    context: &mut NetMDContext,
    track_index: u16,
) -> Result<(), DeviceError> {
    ensure_disc_writable(context).await?;

    let track_count = context
        .interface_mut()
        .track_count()
        .await
        .map_err(|err| DeviceError::ListContent(err.to_string()))?;

    if track_index >= track_count {
        return Err(DeviceError::TrackNumberOutOfRange {
            track_number: track_index + 1,
            track_count,
        });
    }

    Ok(())
}

fn map_rename_track_precondition_error(err: DeviceError) -> DeviceError {
    match err {
        DeviceError::ListContent(message) => DeviceError::RenameTrack(message),
        other => other,
    }
}

fn map_delete_track_precondition_error(err: DeviceError) -> DeviceError {
    match err {
        DeviceError::ListContent(message) => DeviceError::DeleteTrack(message),
        other => other,
    }
}

fn map_erase_precondition_error(err: DeviceError) -> DeviceError {
    match err {
        DeviceError::ListContent(message) => DeviceError::EraseDisc(message),
        other => other,
    }
}

fn map_create_group_precondition_error(err: DeviceError) -> DeviceError {
    match err {
        DeviceError::ListContent(message) => DeviceError::CreateGroup(message),
        other => other,
    }
}

fn compile_group_titles(
    disc: &Disc,
    start_track_index: u16,
    track_count: u16,
    title: &str,
) -> (String, String) {
    let mut groups: Vec<_> = disc
        .groups
        .iter()
        .filter(|group| group.title.is_some() && !group.tracks.is_empty())
        .collect();
    groups.sort_by_key(|group| {
        group
            .tracks
            .iter()
            .map(|track| track.index)
            .min()
            .unwrap_or_default()
    });

    let mut half_width = String::new();
    if !disc.title.trim().is_empty() {
        half_width.push_str(&format!(
            "0;{}//",
            sanitize_half_width_component(&disc.title)
        ));
    }

    let mut full_width = String::new();
    if !disc.full_width_title.trim().is_empty() {
        full_width.push_str(&format!(
            "０；{}／／",
            sanitize_full_width_component(&disc.full_width_title)
        ));
    }

    for group in groups {
        let range = group_range(
            group
                .tracks
                .iter()
                .map(|track| track.index)
                .min()
                .unwrap_or_default(),
            group.tracks.len() as u16,
        );
        half_width.push_str(&format!(
            "{};{}//",
            range,
            sanitize_half_width_component(group.title.as_deref().unwrap_or_default())
        ));
        full_width.push_str(&format!(
            "{}；{}／／",
            half_width_range_to_full_width(&range),
            sanitize_full_width_component(group.full_width_title.as_deref().unwrap_or_default())
        ));
    }

    let range = group_range(start_track_index, track_count);
    half_width.push_str(&format!(
        "{};{}//",
        range,
        sanitize_half_width_component(title)
    ));
    full_width.push_str(&format!("{}；／／", half_width_range_to_full_width(&range)));

    (half_width, full_width)
}

fn group_range(start_track_index: u16, track_count: u16) -> String {
    let start = start_track_index + 1;
    if track_count == 1 {
        start.to_string()
    } else {
        format!("{}-{}", start, start + track_count - 1)
    }
}

fn sanitize_half_width_component(title: &str) -> String {
    title.replace("//", " /")
}

fn sanitize_full_width_component(title: &str) -> String {
    title.replace("／／", "／")
}

fn half_width_range_to_full_width(range: &str) -> String {
    range
        .chars()
        .map(|c| match c {
            '0' => '０',
            '1' => '１',
            '2' => '２',
            '3' => '３',
            '4' => '４',
            '5' => '５',
            '6' => '６',
            '7' => '７',
            '8' => '８',
            '9' => '９',
            '-' => '－',
            other => other,
        })
        .collect()
}

fn ignore_unchanged_title(
    err: minidisc::netmd::interface::InterfaceError,
) -> Result<(), minidisc::netmd::interface::InterfaceError> {
    match err {
        minidisc::netmd::interface::InterfaceError::TitleError => Ok(()),
        other => Err(other),
    }
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

#[cfg(test)]
mod tests {
    use super::{compile_group_titles, group_range};
    use crate::device::{Disc, Group, Track};

    #[test]
    fn group_range_uses_single_track_without_dash() {
        assert_eq!(group_range(0, 1), "1");
        assert_eq!(group_range(2, 3), "3-5");
    }

    #[test]
    fn compile_group_titles_preserves_disc_title_and_existing_groups() {
        let disc = Disc {
            title: "Existing Disc".to_string(),
            full_width_title: "Existing Full".to_string(),
            writable: true,
            write_protected: false,
            used_seconds: None,
            left_seconds: None,
            total_seconds: None,
            track_count: 5,
            groups: vec![
                Group {
                    index: 0,
                    title: None,
                    full_width_title: None,
                    tracks: vec![track(2), track(3), track(4)],
                },
                Group {
                    index: 1,
                    title: Some("Old Group".to_string()),
                    full_width_title: Some("Old Full".to_string()),
                    tracks: vec![track(0), track(1)],
                },
            ],
        };

        let (half_width, full_width) = compile_group_titles(&disc, 2, 3, "New Group");

        assert_eq!(
            half_width,
            "0;Existing Disc//1-2;Old Group//3-5;New Group//"
        );
        assert_eq!(
            full_width,
            "０；Existing Full／／１－２；Old Full／／３－５；／／"
        );
    }

    fn track(index: u16) -> Track {
        Track {
            index,
            title: None,
            duration_seconds: None,
            channels: None,
            codec: None,
            protected: None,
        }
    }
}
