use cubist_proxy::FatalErr;
use reqwest::Url;
use std::{path::PathBuf, process::ExitStatus};
use thiserror::Error;
use url;

/// Errors raised by this library.
#[derive(Debug, Error)]
pub enum Error {
    /// Error raised when we can't resolve hostname.
    #[error("Could not resolve hostname {0:?}")]
    CannotResolveHostname(String),
    /// Error raised when we can't download node binaries for particular platform.
    #[error("Unsupported platform: {name} is not supported on platform {os}-{arch}. It is only supported on {supported}.")]
    UnsupportedPlatformError {
        name: String,
        os: String,
        arch: String,
        supported: String,
    },
    /// Error raised when the number of specified binaries and hashes do not match.
    #[error("Number of specified binaries ({num_binaries}) and hashes ({num_hashes}) must match")]
    MismatchedBinariesAndHashes {
        num_binaries: usize,
        num_hashes: usize,
    },
    /// Error raised when no binaries are specified for a resource.
    #[error("No binaries specified for {0}")]
    MissingBinaries(String),
    /// Error raised when e.g., we fail to start the provider node.
    #[error(transparent)]
    ProviderError(#[from] ProviderError),
    /// Error raised when e.g., we fail to start the provider node.
    #[error(transparent)]
    EthersProviderError(#[from] ethers::providers::ProviderError),
    /// Error raised when downloading failes
    #[error(transparent)]
    DownloadError(#[from] DownloadError),
    /// Generic filesystem error
    #[error("{0}. Path: {1}")]
    FsError(&'static str, PathBuf, #[source] std::io::Error),
    /// Json error
    #[error("{0}. Path: {1:?}")]
    JsonError(&'static str, Option<PathBuf>, #[source] serde_json::Error),
    /// Generic IO error
    #[error("IO error")]
    IOError(#[source] std::io::Error),
    #[error("Server timeout: {0} failed to start: {1}")]
    ServerTimeout(String, String),
    #[error("Proxy error")]
    ProxyError(#[from] FatalErr),
    #[error("Config error")]
    ConfigError(#[from] cubist_config::ConfigError),
    /// Error raised when a server is not running when we expect it to.
    #[error("{0} process terminated unexpectedly, exit status {1}")]
    ProcessTerminated(String, ExitStatus),
}

/// Provider-specific errors
#[derive(Debug, Error)]
pub enum ProviderError {
    /// Error raised when setting up the provider failed
    #[error("Setting up provider failed: {0}")]
    SetupError(String),
    /// Error raised when starting the provider node(s) failed
    #[error("Starting node failed: {0}")]
    StartError(String),
    /// Error raised when starting the provider node(s) failed
    #[error("Timeout while connecting to server: {0}")]
    ServerTimeout(String),
    /// Error raised while trying to parse a URL
    #[error("Error parsing URL: {0}")]
    UrlParseError(#[from] url::ParseError),
}

/// Download(able) errors
#[derive(Debug, Error)]
pub enum DownloadError {
    /// Error raised when downloaded file is missing
    #[error("Downloaded file {0} is missing")]
    MissingDownloadedFile(PathBuf),
    /// Error raised when the file hash is incorrect
    #[error("Hash value for {file} does not match expected value: {expected} != {actual}")]
    IncorrectHash {
        file: PathBuf,
        expected: blake3::Hash,
        actual: blake3::Hash,
    },
    /// Error raised because the download is malformed in some way
    #[error("Malformed download: {0}")]
    MalformedDownload(String),
    /// Error raised because of a request error
    #[error("Request error; url = {0}")]
    RequestError(Url, #[source] reqwest::Error),
    /// Error raised when e.g., we fail to save a downloaded file.
    #[error("Failed to save downloaded file to {0}")]
    SaveError(PathBuf, #[source] std::io::Error),
}

/// Result type for this library.
pub type Result<T, E = Error> = core::result::Result<T, E>;

#[macro_export]
macro_rules! setup_error {
    ($msg:literal $(,)?) => {
        return $crate::error::ProviderError::SetupError($msg)
    };
    ($fmt:expr, $($arg:tt)*) => {
        return $crate::error::ProviderError::SetupError(format!($fmt, $($arg)*))
    };
}

#[macro_export]
macro_rules! start_error {
    ($msg:literal $(,)?) => {
        return $crate::error::ProviderError::StartError($msg)
    };
    ($fmt:expr, $($arg:tt)*) => {
        return $crate::error::ProviderError::StartError(format!($fmt, $($arg)*))
    };
}
