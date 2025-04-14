use std::{path::Path, time::Duration};

use anyhow::{anyhow, Context, Result};
use aws_sdk_s3::primitives::ByteStream;

use crate::{error::RequestError, persistent_config::ChannelsConfig};

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
