use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all channels.
    ListChannels {
        /// The S3 bucket to upload the content to.
        bucket: String,
    },
    /// Show the channel details.
    ShowChannel {
        /// The S3 bucket to upload the content to.
        bucket: String,

        /// The channel to publish for.
        channel: String,
    },
    Publish {
        /// The S3 bucket to upload the content to.
        bucket: String,

        /// The channel to publish for.
        channel: String,

        /// The file to upload.
        file: PathBuf,

        /// Create the channel if it doesn't exist.
        #[arg(long, default_value_t = false)]
        create: bool,
    },
}

/// A program to serve a S3 bucket via the Nix Lockable Tarball Protocol.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    commands: Commands,
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.commands {
        Commands::ListChannels { bucket: _ } => todo!(),
        Commands::ShowChannel {
            bucket: _,
            channel: _,
        } => todo!(),
        Commands::Publish {
            bucket: _,
            channel: _,
            file: _,
            create: _,
        } => todo!(),
    }

    #[allow(unreachable_code)]
    Ok(())
}
