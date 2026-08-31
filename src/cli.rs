use clap::{Parser, Subcommand};

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

    /// Show Linux USB permission diagnostics and udev setup guidance.
    Doctor,
}
