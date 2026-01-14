use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use s3_nix_channel::persistent::Client;

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
    },
}

/// A program to serve a S3 bucket via the Nix Lockable Tarball Protocol.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    commands: Commands,
}

impl Args {
    fn bucket(&self) -> &str {
        match &self.commands {
            Commands::ListChannels { bucket }
            | Commands::ShowChannel { bucket, channel: _ }
            | Commands::Publish {
                bucket,
                channel: _,
                file: _,
            } => bucket,
        }
    }
}

async fn list_channels(s3_client: &Client) -> Result<()> {
    let config = s3_client.load_channels_config().await?;

    config
        .channels()
        .for_each(|(name, cfg)| println!("{name} ({})", cfg.file_extension));

    Ok(())
}

async fn show_channel(s3_client: &Client, channel: &str) -> Result<()> {
    let config = s3_client.load_channels_config().await?;

    println!(
        "Latest: {}",
        config
            .channel(channel)
            .context("No such channel")?
            .latest
            .as_deref()
            .unwrap_or("(nothing yet)")
    );

    Ok(())
}

async fn publish(s3_client: &Client, channel: &str, file: &Path) -> Result<()> {
    s3_client
        .update_channel(channel, file)
        .await
        .context("Failed to update channel")?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let s3_client = Client::new_from_env(args.bucket()).await?;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    match args.commands {
        Commands::ListChannels { bucket: _ } => list_channels(&s3_client).await?,
        Commands::ShowChannel { bucket: _, channel } => show_channel(&s3_client, &channel).await?,
        Commands::Publish {
            bucket: _,
            channel,
            file,
        } => publish(&s3_client, &channel, &file).await?,
    }

    Ok(())
}
