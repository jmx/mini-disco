use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "mini-disco")]
#[command(about = "Work with NetMD MiniDisc devices from a Linux terminal")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Connect to one NetMD device and list the inserted disc contents.
    List {
        /// Print machine-readable JSON instead of a human table.
        #[arg(long)]
        json: bool,
    },

    /// Convert an MP3, FLAC, WAV, or other ffmpeg-supported audio file and upload it.
    Upload {
        /// Source audio file supported by ffmpeg, such as MP3, FLAC, WAV, AAC, or Ogg Vorbis.
        path: PathBuf,

        /// Target recording format. Converted uploads default to SP.
        #[arg(long, value_enum, default_value = "sp")]
        format: RawFormat,

        /// Track title to write after transfer. Defaults to the input file name stem.
        #[arg(long)]
        title: Option<String>,
    },

    /// Upload prepared raw audio bytes to the inserted disc.
    UploadRaw {
        /// Prepared raw audio file.
        path: PathBuf,

        /// Raw audio wire format: sp is big-endian 16-bit stereo PCM; lp2/lp105/lp4 are headerless ATRAC3 frames.
        #[arg(long, value_enum)]
        format: RawFormat,

        /// Track title to write after transfer. Defaults to the input file name stem.
        #[arg(long)]
        title: Option<String>,
    },

    /// Convert an audio file into prepared raw bytes without writing a disc.
    Convert {
        /// Source audio file supported by ffmpeg, such as MP3, FLAC, WAV, AAC, or Ogg Vorbis.
        input: PathBuf,

        /// Output raw file to write.
        output: PathBuf,

        /// Target raw format.
        #[arg(long, value_enum, default_value = "sp")]
        format: RawFormat,
    },

    /// Rename the inserted disc while preserving group metadata.
    RenameDisc {
        /// New disc title. Pass an empty string to clear the title.
        title: String,
    },

    /// Rename one track by its displayed 1-based track number.
    RenameTrack {
        /// Track number as shown by `list`.
        track: u16,

        /// New track title. Pass an empty string to clear the title.
        title: String,
    },

    /// Delete one track by its displayed 1-based track number.
    DeleteTrack {
        /// Track number as shown by `list`.
        track: u16,
    },

    /// Start or resume playback on the attached NetMD device.
    Play,

    /// Pause playback on the attached NetMD device.
    Pause,

    /// Stop playback on the attached NetMD device.
    Stop,

    /// Skip to the next track on the attached NetMD device.
    Next,

    /// Skip to the previous track on the attached NetMD device.
    Prev,

    /// Show Linux USB permission diagnostics and udev setup guidance.
    Doctor,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RawFormat {
    Sp,
    Lp2,
    Lp105,
    Lp4,
}
