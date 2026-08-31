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

    /// Convert an audio file with ffmpeg and upload it to the inserted disc.
    Upload {
        /// Source audio file supported by ffmpeg.
        path: PathBuf,

        /// Target recording format. Only sp is converted in this iteration.
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

        /// Raw audio wire format: sp is big-endian 16-bit stereo PCM; lp2/lp4 are headerless ATRAC3 frames.
        #[arg(long, value_enum)]
        format: RawFormat,

        /// Track title to write after transfer. Defaults to the input file name stem.
        #[arg(long)]
        title: Option<String>,
    },

    /// Show Linux USB permission diagnostics and udev setup guidance.
    Doctor,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RawFormat {
    Sp,
    Lp2,
    Lp4,
}
