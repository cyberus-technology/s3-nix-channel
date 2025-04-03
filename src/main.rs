mod error;
mod persistent_config;

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use arc_swap::ArcSwap;
use aws_sdk_s3::presigning::PresigningConfig;
use axum::{
    extract::{Path, Request, State},
    http::{header::LINK, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{self, IntoResponse, Redirect},
    routing::get,
    Router,
};

use clap::Parser;
use error::RequestError;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use persistent_config::ChannelsConfig;
use tokio::time::interval;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

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

    /// Enable authentication using JWT by specifying the public key
    /// for token verification.
    #[arg(long)]
    jwt_pem: Option<PathBuf>,
}

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

#[derive(Debug, serde::Deserialize)]
struct Claims {
    // We need nothing.
}

/// Extract the HTTP Basic Authorization password.
fn extract_auth_password(headers: &HeaderMap) -> Option<String> {
    use base64::prelude::*;

    // Get the Authorization header value
    let header = headers.get("Authorization")?;
    let header_value = header.to_str().ok()?;

    let credentials = header_value.strip_prefix("Basic ")?.to_owned();

    let credentials = String::from_utf8(BASE64_STANDARD.decode(&credentials).ok()?).ok()?;

    let pw = credentials
        .split_once(':')
        .map(|(_user, password)| password.to_owned());

    pw
}

/// If a JWT public key is available, make sure that each request is authorized.
async fn auth_middleware(
    State(decoding_key): State<DecodingKey>,
    request: Request,
    next: Next,
) -> response::Response {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_nbf = true;

    // TODO What we validate in the claims should be configurable. For
    // now we just check whether the token is signed and valid.
    validation.validate_aud = false;
    validation.set_required_spec_claims(&["exp"]);

    match extract_auth_password(request.headers())
        .ok_or_else(|| anyhow!("Missing Authorization header"))
        .and_then(|jwt_str| {
            jsonwebtoken::decode::<Claims>(&jwt_str, &decoding_key, &validation)
                .context("Failed to decode token")
        }) {
        Ok(claim) => {
            debug!("Claim {:?}", claim)
        }
        Err(e) => {
            info!("JWT validation error: {e}");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    next.run(request).await
}

async fn log_request_middleware(req: Request, next: Next) -> response::Response {
    let path = req.uri().path().to_owned();
    let method = req.method().clone();

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let duration = start.elapsed();

    let status = response.status();

    info!(
        "Request: {} {} -> {} ({:?})",
        method,
        path,
        status.as_u16(),
        duration
    );

    response
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

    // The first tick completes immediately, so we skip it.
    interval.tick().await;

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
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder().parse("info,tarball_serve=debug")?,
        )
        .init();

    let amzn_config = aws_config::load_from_env().await;
    let s3_config = aws_sdk_s3::config::Builder::from(&amzn_config)
        // TODO For minio compat. Should this be configurable?
        .force_path_style(true)
        .build();
    let s3_client = aws_sdk_s3::Client::from_conf(s3_config);

    let channels = ChannelsConfig::from_s3_bucket(&s3_client, &args.bucket).await?;
    let jwt_public_key = args
        .jwt_pem
        .map(|pem_file| {
            std::fs::read(&pem_file).with_context(|| {
                format!("Failed to read public key PEM from {}", pem_file.display())
            })
        })
        // Be sure to handle the I/O error, so we don't accidentally
        // misinterpret "couldn't read file" as "there is no public
        // key", which would make the service accessible without
        // authentication.
        .transpose()?
        .map(|pem_data| DecodingKey::from_rsa_pem(&pem_data).context("Failed to decode public key"))
        .transpose()?;

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
    let mut app = Router::new()
        .route("/channel/{*path}", get(handle_channel))
        .route("/permanent/{*path}", get(handle_persistent))
        .with_state(config);

    if let Some(jwt_public_key) = jwt_public_key {
        let auth_layer = middleware::from_fn_with_state(jwt_public_key, auth_middleware);

        app = app.layer(auth_layer);
    }

    // Layer logging last, so we can see authentication failures as well.
    app = app
	.layer(middleware::from_fn(log_request_middleware))
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request| {
                tracing::debug_span!("request", method = %request.method(), uri = %request.uri())
            }),
        );

    // TODO Accept socket from systemd. We could run without credentials.
    info!("Listening on {}", &args.listen);
    let listener = tokio::net::TcpListener::bind(&args.listen).await?;

    tokio::task::spawn_blocking(|| {
        use sd_notify::{notify, NotifyState};
        if let Err(e) = notify(true, &[NotifyState::Ready]) {
            warn!("Failed to notify systemd: {e}");
        } else {
            debug!("Notified systemd that we are ready to serve!");
        }
    })
    .await?;

    axum::serve(listener, app).await?;

    Ok(())
}
