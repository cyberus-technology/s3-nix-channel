mod error;

use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
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

/// A program to serve a S3 bucket via the Nix Lockable Tarball Protocol.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// URL of the S3 endpoint
    #[arg(long)]
    endpoint: String,

    /// Name of the person to greet
    #[arg(long)]
    bucket: String,
}

#[derive(Debug, Clone)]
struct Config {
    s3_client: aws_sdk_s3::Client,
    bucket: String,
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

async fn find_newest_file(config: &Config) -> Result<String> {
    let objects = config
        .s3_client
        .list_objects_v2()
        .bucket(&config.bucket)
        .send()
        .await?;

    // Find the object with the most recent LastModified timestamp
    let newest_object = objects
        .contents
        .unwrap_or_default()
        .into_iter()
        .max_by_key(|obj| obj.last_modified.clone())
        .ok_or_else(|| anyhow!("No files found"))?;

    newest_object.key().context("No key?").map(str::to_owned)
}

#[axum::debug_handler]
async fn handle_current_tarxz_file(
    State(config): State<Arc<Config>>,
) -> Result<impl IntoResponse, RequestError> {
    let mut headers = HeaderMap::new();

    // TODO This is slow.
    let newest_object = find_newest_file(&config).await.unwrap();

    headers.insert(
        LINK,
        HeaderValue::from_str(&format!(
            // The Lockable HTTP Tarball Protocol. See:
            // https://nix.dev/manual/nix/2.25/protocols/tarball-fetcher
            //
            // TODO The root URL should be configurable.
            "<http://localhost:3000/permanent/{newest_object}>; rel=\"immutable\""
        ))
        .map_err(|_e| RequestError::Unknown)?,
    );

    let signed_url = sign_request(&config, &newest_object).await.unwrap();

    Ok((headers, Redirect::temporary(&signed_url)).into_response())
}

#[axum::debug_handler]
async fn handle_tarxz_file(
    Path(path): Path<String>,
    State(config): State<Arc<Config>>,
) -> Result<impl IntoResponse, RequestError> {
    // TODO Do some sanity checking of path.
    Ok(Redirect::temporary(&sign_request(&config, &path).await?))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // TODO Port to simpler S3 library to avoid tons of dependencies. rust-s3?
    let amzn_config = aws_config::load_from_env().await;

    let config = Config {
        s3_client: aws_sdk_s3::Client::from_conf(
            aws_sdk_s3::config::Builder::from(&amzn_config)
                .endpoint_url(&args.endpoint)
                .build(),
        ),
        bucket: args.bucket,
    };

    // build our application with a single route
    let app = Router::new()
        .route("/current.tar.xz", get(handle_current_tarxz_file))
        .route("/permanent/{*path}", get(handle_tarxz_file))
        .with_state(Arc::new(config));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
