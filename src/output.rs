use crate::device::{DeviceSnapshot, Disc, Group, Track};
use anyhow::Result;

pub fn print_disc_json(snapshot: &DeviceSnapshot) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(snapshot)?);
    Ok(())
}

pub fn print_disc_human(snapshot: &DeviceSnapshot) {
    println!(
        "Device: {}",
        empty_fallback(&snapshot.device_name, "Unknown NetMD device")
    );
    println!(
        "USB: {:04x}:{:04x}",
        snapshot.vendor_id, snapshot.product_id
    );
    if let Some(recording_parameters) = &snapshot.recording_parameters {
        println!(
            "Recording parameters: {}",
            format_hex_bytes(recording_parameters)
        );
    }
    print_disc_summary(&snapshot.disc);

    for group in &snapshot.disc.groups {
        print_group(group);
    }
}

fn print_disc_summary(disc: &Disc) {
    println!("Disc: {}", empty_fallback(&disc.title, "Untitled Disc"));
    println!("Tracks: {}", disc.track_count);

    if let (Some(used), Some(left), Some(total)) =
        (disc.used_seconds, disc.left_seconds, disc.total_seconds)
    {
        println!(
            "Capacity: {} used, {} left, {} total",
            format_duration(used),
            format_duration(left),
            format_duration(total)
        );
    }

    if disc.write_protected {
        println!("Write protection: on");
    } else if disc.writable {
        println!("Write protection: off");
    }

    println!();
}

fn print_group(group: &Group) {
    if let Some(title) = &group.title {
        println!(
            "[Group {}] {}",
            group.index,
            empty_fallback(title, "Untitled Group")
        );
    }

    for track in &group.tracks {
        print_track(track);
    }
}

fn print_track(track: &Track) {
    let title = track.title.as_deref().unwrap_or("Untitled Track");
    let duration = track
        .duration_seconds
        .map(format_duration)
        .unwrap_or_else(|| "--:--".to_string());
    let codec = track.codec.as_deref().unwrap_or("unknown");

    let channels = match track.channels {
        Some(1) => "mono",
        Some(2) => "stereo",
        Some(_) => "multi",
        None => "unknown",
    };

    println!(
        "{:>3}. {:<8} {:<8} {:<8} {}",
        track.index + 1,
        duration,
        codec,
        channels,
        title
    );
}

fn format_duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn format_hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("0x{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn empty_fallback<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}
