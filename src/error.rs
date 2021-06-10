use thiserror::Error;

/// Describes the various errors that can be returned from the client
#[derive(Error, Debug)]
pub enum ClientError {
    /// Indicates that the given URL is invalid, contains the underlying parsing error
    #[error("Invalid URL given: {0:?}")]
    InvalidUrl(#[from] url::ParseError),
    /// Invalid configuration was given to the client
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    /// IO errors from interacting with the file system
    #[error("Error while performing IO operation: {0:?}")]
    Io(#[from] std::io::Error),
    /// Invalid JSON serialization or deserialization serializing an object to a request
    /// or from a response
    #[error("Invalid JSON: {0:?}")]
    SerializationError(#[from] serde_json::Error),
    /// There was a problem with the http client. This is likely not a user issue. Contains the
    /// underlying error
    #[error("Error creating request: {0:?}")]
    HttpClientError(#[from] reqwest::Error),

    // API errors
    #[error("Invalid request (status code {status_code:?}): {message:?}")]
    InvalidRequest {
        status_code: reqwest::StatusCode,
        message: Option<String>,
    },
    /// A server error was encountered. Contains an optional message from the server
    #[error("Server has encountered an error: {0:?}")]
    ServerError(Option<String>),
    /// Invalid credentials were used or user does not have access to the requested resource. This
    /// is only valid if the server supports authentication and/or permissions
    #[error("User has invalid credentials or is not authorized to access the requested resource")]
    Unauthorized,

    /// A catch-all for uncategorized errors. Contains an error message describing the underlying
    /// issue
    #[error("{0}")]
    Other(String),
}

impl From<std::convert::Infallible> for ClientError {
    fn from(_: std::convert::Infallible) -> Self {
        // Doesn't matter what we return as Infallible cannot happen
        ClientError::Other("Shouldn't happen".to_string())
    }
}
