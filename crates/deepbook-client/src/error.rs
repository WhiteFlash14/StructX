use thiserror::Error;

pub type Result<T> = std::result::Result<T, DeepBookClientError>;

#[derive(Debug, Error)]
pub enum DeepBookClientError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("endpoint returned non-success status {status}: {body}")]
    HttpStatus { status: reqwest::StatusCode, body: String },

    #[error("failed to deserialize response from {endpoint}: {source}")]
    Decode {
        endpoint: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("unexpected response shape for endpoint {endpoint}: {message}")]
    UnexpectedShape { endpoint: String, message: String },
}
