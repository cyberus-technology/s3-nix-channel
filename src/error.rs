use axum::{http::StatusCode, response::IntoResponse};

#[derive(thiserror::Error, Debug)]
pub enum RequestError {
    #[error("Failed to presign request for object {object_key:?}")]
    PresignFailure { object_key: String },
    #[error("Failed to create presign configuration")]
    PresignConfigFailure,
    #[error("Unknown error")]
    Unknown,
}

impl IntoResponse for RequestError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Request failed: {}", &self),
        )
            .into_response()
    }
}
