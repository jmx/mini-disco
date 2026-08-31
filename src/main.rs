mod app;
mod cli;
mod device;
mod netmd;
mod output;
mod udev;

use clap::Parser;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match app::run(cli).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}
