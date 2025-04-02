use std::collections::BTreeMap;

use anyhow::{Context, Result};
use axum::body::Bytes;
use serde::Deserialize;
use tracing::{debug, error, info};

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
struct PersistentChannelConfig {
    /// The latest element in the channel. If this is foo, users can download it as channel/foo.tar.gz.
    latest: String,
}

/// The list of channels we know about and their latest object keys.
#[derive(Debug, Default, Clone)]
pub struct ChannelsConfig {
    /// A mapping from channel name to latest object key.
    channels: BTreeMap<String, String>,
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
    pub fn latest_object_key(&self, channel_name: &str) -> Option<&str> {
        self.channels.get(channel_name).map(|s| s.as_str())
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
                    serde_json::from_slice::<PersistentChannelConfig>(&bytes)
                        .context("Failed to deserialize channel configuration")
                })
            {
                info!("Channel {channel_name} points to: {}", config.latest);
                channels_config.channels.insert(channel_name, config.latest);
            } else {
                error!("Configured channel {channel_name:?} has no corresponding {config_file} in the bucket. Ignoring!");
                continue;
            }
        }

        Ok(channels_config)
    }
}
