use axum::{http::StatusCode, response::IntoResponse};

#[derive(thiserror::Error, Debug)]
pub enum RequestError {
    #[error("Failed to presign request for object {object_key:?}")]
    PresignFailure { object_key: String },
    #[error("Failed to create presign configuration")]
    PresignConfigFailure,
    #[error("There is no such channel: {channel_name:?}")]
    NoSuchChannel { channel_name: String },
    #[error("Only requests for .tar.xz files are supported: {file_name:?}")]
    InvalidFile { file_name: String },
    #[error("Unknown error")]
    Unknown,
}

impl IntoResponse for RequestError {
    fn into_response(self) -> axum::response::Response {
        (
            match self {
                RequestError::NoSuchChannel { channel_name: _ } => StatusCode::NOT_FOUND,
                RequestError::InvalidFile { file_name: _ } => StatusCode::BAD_REQUEST,
                RequestError::PresignConfigFailure
                | RequestError::PresignFailure { object_key: _ }
                | RequestError::Unknown => StatusCode::INTERNAL_SERVER_ERROR,
            },
            format!("{}", &self),
        )
            .into_response()
    }
}
