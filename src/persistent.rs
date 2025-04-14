use std::{
    collections::BTreeMap,
    {path::Path, time::Duration},
};

use anyhow::{anyhow, Context, Result};
use aws_sdk_s3::primitives::ByteStream;
use axum::body::Bytes;
use serde::Deserialize;
use tracing::{debug, error, info};

use crate::error::RequestError;

/// The persistent configuration that lives in the S3 bucket as
/// /channels.json.
#[derive(Deserialize, Debug, Clone)]
struct PersistentChannelsConfig {
    /// The list of all channels we serve. Each channel needs a
    /// corresponding <channel>.json file for configuration in the
    /// bucket.
    channels: Vec<String>,
}

/// The persistent configuration of a single channel.
#[derive(Deserialize, Debug, Clone)]
pub struct ChannelConfig {
    /// The latest element in the channel. If this is foo, users can download it as channel/foo.tar.gz.
    pub latest: String,
}

/// The list of channels we know about and their latest object keys.
#[derive(Debug, Default, Clone)]
pub struct ChannelsConfig {
    /// A mapping from channel name to latest object key.
    channels: BTreeMap<String, ChannelConfig>,
}

/// Read a file from the bucket..
async fn read_file(
    s3_client: &aws_sdk_s3::Client,
    bucket: &str,
    object_key: &str,
) -> Result<Bytes> {
    let response = s3_client
        .get_object()
        .bucket(bucket)
        .key(object_key)
        .send()
        .await
        // TODO Better error.
        .with_context(|| format!("Failed to read: {object_key}"))?;

    Ok(response.body.collect().await?.into_bytes())
}

impl ChannelsConfig {
    pub fn channels(&self) -> impl Iterator<Item = &str> {
        self.channels.keys().map(|s| s.as_ref())
    }

    pub fn channel(&self, channel_name: &str) -> Option<ChannelConfig> {
        self.channels.get(channel_name).map(|c| c.clone())
    }

    /// Read the channels configuration from the bucket.
    pub async fn from_s3_bucket(
        s3_client: &aws_sdk_s3::Client,
        bucket: &str,
    ) -> Result<ChannelsConfig> {
        let persistent_config: PersistentChannelsConfig =
            serde_json::from_slice(&read_file(s3_client, bucket, "channels.json").await?)
                .context("Failed to deserialize channels.json")?;

        debug!("Loaded channel config: {persistent_config:?}");

        let mut channels_config = ChannelsConfig::default();

        for channel_name in persistent_config.channels {
            let config_file = format!("{channel_name}.json");
            if let Ok(config) = read_file(s3_client, bucket, &config_file)
                .await
                .context("Failed to read channel config")
                .and_then(|bytes| {
                    serde_json::from_slice::<ChannelConfig>(&bytes)
                        .context("Failed to deserialize channel configuration")
                })
            {
                info!("Channel {channel_name} points to: {}", config.latest);
                channels_config.channels.insert(
                    channel_name,
                    ChannelConfig {
                        latest: config.latest,
                    },
                );
            } else {
                error!("Configured channel {channel_name:?} has no corresponding {config_file} in the bucket. Ignoring!");
                continue;
            }
        }

        Ok(channels_config)
    }
}

pub struct Client {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl Client {
    /// Open an S3 client with configuration from the environment.
    pub async fn new_from_env(bucket: &str) -> Result<Client> {
        let amzn_config = aws_config::load_from_env().await;
        let s3_config = aws_sdk_s3::config::Builder::from(&amzn_config)
            // TODO For minio compat. Should this be configurable?
            .force_path_style(true)
            .build();

        Ok(Self {
            client: aws_sdk_s3::Client::from_conf(s3_config),
            bucket: bucket.to_owned(),
        })
    }

    pub async fn load_channels_config(&self) -> Result<ChannelsConfig> {
        ChannelsConfig::from_s3_bucket(&self.client, &self.bucket).await
    }

    pub async fn sign_request(&self, object_key: &str) -> Result<String, RequestError> {
        use aws_sdk_s3::presigning::PresigningConfig;

        Ok(self
            .client
            .get_object()
            .bucket(&self.bucket)
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

    /// Upload a tarball to the persistent store. Doesn't update any channel.
    pub async fn upload_tarball(&self, object_key: &str, file: &Path) -> Result<()> {
        if !object_key.ends_with(".tar.xz") {
            return Err(anyhow!(
                "Invalid file ending. Only .tar.xz is supported: {object_key}"
            ));
        }

        let data = ByteStream::read_from()
            .path(file)
            .build()
            .await
            .context("Failed to read input file")?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(object_key)
            .body(data)
            .send()
            .await
            .context("Failed to upload file")?;

        Ok(())
    }
}
