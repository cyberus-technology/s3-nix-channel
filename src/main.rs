mod error;
mod persistent_config;

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use arc_swap::ArcSwap;
use aws_sdk_s3::presigning::PresigningConfig;
use axum::{
    extract::{Path, State},
    http::{header::LINK, HeaderMap, HeaderValue},
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use clap::Parser;
use error::RequestError;
use persistent_config::ChannelsConfig;
use tokio::time::interval;
use tracing::{error, info};

/// A program to serve a S3 bucket via the Nix Lockable Tarball Protocol.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The S3 bucket to serve the content from.
    #[arg(long)]
    bucket: String,

    /// The base URL of the service.
    ///
    /// If you want to serve objects from
    /// https://foo.com/permanent/123.tar.xz, you need to specify
    /// https://foo.com here.
    #[arg(long)]
    base_url: String,

    /// The interval in seconds for updating the configuration from
    /// the config.json file in the bucket.
    #[arg(long, default_value_t = 3600)]
    config_update_seconds: u64,

    /// What IP and port to listen on. Specify as <IP>:<port>.
    #[arg(long, default_value = "localhost:3000")]
    listen: String,
}

#[derive(Debug)]
struct Config {
    s3_client: aws_sdk_s3::Client,
    bucket: String,
    base_url: String,
    update_interval: Duration,
    channels: ArcSwap<ChannelsConfig>,
}

impl Config {
    /// Return the latest object key for a given channel, if there is one.
    fn latest_object_key(&self, channel_name: &str) -> Option<String> {
        let channels = self.channels.load();

        // The config may be updated concurrently. We can't hand out a
        // reference.
        channels
            .latest_object_key(channel_name)
            .map(|x| x.to_owned())
    }
}

async fn sign_request(config: &Config, object_key: &str) -> Result<String, RequestError> {
    Ok(config
        .s3_client
        .get_object()
        .bucket(&config.bucket)
        .key(object_key)
        // TODO Should expiration be configurable?
        .presigned(
            PresigningConfig::expires_in(Duration::from_secs(600))
                .map_err(|_e| RequestError::PresignConfigFailure)?,
        )
        .await
        .map_err(|_e| RequestError::PresignFailure {
            object_key: object_key.to_owned(),
        })?
        .uri()
        .to_string())
}

/// Redirect to the latest tarball of the requested channel.
async fn handle_channel(
    Path(path): Path<String>,
    State(config): State<Arc<Config>>,
) -> Result<impl IntoResponse, RequestError> {
    let channel_name = path
        .strip_suffix(".tar.xz")
        .ok_or_else(|| RequestError::InvalidFile {
            file_name: path.clone(),
        })?;

    let latest_object =
        config
            .latest_object_key(channel_name)
            .ok_or_else(|| RequestError::NoSuchChannel {
                channel_name: channel_name.to_owned(),
            })?;

    let mut headers = HeaderMap::new();

    // The Lockable HTTP Tarball Protocol. See:
    // https://nix.dev/manual/nix/2.25/protocols/tarball-fetcher
    headers.insert(
        LINK,
        HeaderValue::from_str(&format!(
            "<{}/permanent/{latest_object}.tar.xz>; rel=\"immutable\"",
            config.base_url
        ))
        .map_err(|_e| RequestError::Unknown)?,
    );

    Ok((
        headers,
        Redirect::temporary(&sign_request(&config, &format!("{latest_object}.tar.xz")).await?),
    ))
}

/// Forward a request to the backing store.
async fn handle_persistent(
    Path(path): Path<String>,
    State(config): State<Arc<Config>>,
) -> Result<impl IntoResponse, RequestError> {
    if !path.ends_with(".tar.xz") {
        return Err(RequestError::InvalidFile {
            file_name: path.clone(),
        });
    }

    Ok(Redirect::temporary(&sign_request(&config, &path).await?))
}

/// Poll the bucket for changes of the configuration.
async fn poll_config_file(state: &Config) {
    let mut interval = interval(state.update_interval);
    loop {
        interval.tick().await;

        let new_channels =
            match ChannelsConfig::from_s3_bucket(&state.s3_client, &state.bucket).await {
                Ok(channels) => channels,
                Err(e) => {
                    error!("Failed to load new config (will try again later): {e}");
                    continue;
                }
            };

        state.channels.store(Arc::new(new_channels));
        info!("Successfully refreshed channel state.")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let amzn_config = aws_config::load_from_env().await;
    let s3_client = aws_sdk_s3::Client::from_conf(aws_sdk_s3::config::Config::from(&amzn_config));

    let channels = ChannelsConfig::from_s3_bucket(&s3_client, &args.bucket).await?;
    let config = Arc::new(Config {
        s3_client,
        bucket: args.bucket,
        base_url: args.base_url,
        update_interval: Duration::from_secs(args.config_update_seconds),
        channels: ArcSwap::new(Arc::new(channels)),
    });

    // Reload the config periodically.
    let update_state = config.clone();
    tokio::spawn(async move {
        poll_config_file(&update_state).await;
    });

    // TODO Add proper logging of requests.
    let app = Router::new()
        .route("/channel/{*path}", get(handle_channel))
        .route("/permanent/{*path}", get(handle_persistent))
        .with_state(config);

    info!("Listening on {}", &args.listen);
    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
