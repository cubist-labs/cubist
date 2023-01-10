// Copyright 2022 Riad S. Wahby <r@cubist.dev> and the Cubist developers
//
// This file is part of cubist-proxy.
//
// See LICENSE for licensing terms. This file may not be copied,
// modified, or distributed except according to those terms.

/*! Library-wide error types
 *
 *  Each "pipeline type" (i.e., JsonRpc, HttpRr) has a corresponding error
 *  type. The currently defined error types for pipelines are:
 *
 *  - JsonRpcErr represents an error processing a request in a pipeline that
 *    carries JSON-RPC values. This is the error type that a Pair<S, R>
 *    carries by default.
 *
 *  - HttpRrErr represents an error processing a request in a pipeline that
 *    carries HTTP Request/Response values.
 *
 *  In addition, there's an over-arching error type:
 *
 *  - FatalErr represents an unrecoverable error. Pair<S, R>'s Sink produces this
 *    error if something goes wrong when writing data to the sink, for example.
 *
 *  Because of the behavior of several combinators in StreamExt and SinkExt
 *  (notably, StreamExt::forward), it is important that we **do not implement**
 *  automatic JsonRpcErr -> FatalErr or FatalErr -> JsonRpcErr conversions (and likewise
 *  for OffchainErr and any future errors of this sort). The issue is that
 *  StreamExt::forward assumes a Stream<Result<T, E>> will be writing into
 *  a Sink<T, Error = E>, so if you accidentally supply a Sink<T, Error = E>,
 *  StreamExt::forward will eat errors rather than passing them downstream.
 *  A Sink with Error != E will cause a type error in this case.
 *
 *  Notes:
 *
 *  1. If you want to write from a Stream<Item = Result<T, E>> into a
 *  Sink<Result<T, E>> using StreamExt::forward, the right way to do it is
 *  `stream.map(Ok).forward(sink)`. This ensures that the Result value, not
 *  just the T values, go into the sink.
 *
 *  2.  JsonRpcErr and HttpRrErr can carry a FatalErr along. This is
 *  to make it possible for Stream adaptors to generate fatal errors in the
 *  pipeline. The idea is that the FatalErr gets carried along until it hits
 *  `Offchain` (or possibly other elements in a pipeline), at which point the
 *  pipeline gets killed.
 */

use ethers::signers::WalletError;
use futures::channel::mpsc;
use hyper::{http::StatusCode, Response};
use serde_json::Error as JsonParseError;
use std::sync::Arc;
use thiserror::Error;
use tokio_tungstenite::tungstenite::Error as WsError;
use url::ParseError;

use crate::{
    jrpc::{parse_error, Error as JrpcError},
    offchain::{json_string, response},
};

// implement From for something that we stuff into an Arc
macro_rules! into_arc_variant {
    ($e: ty, $t: ty, $v: ident) => {
        impl From<$t> for $e {
            fn from(other: $t) -> Self {
                Self::$v(Arc::new(other))
            }
        }
    };
}

/// Failure of an entire pipeline.
#[derive(Clone, Debug, Error)]
pub enum FatalErr {
    /// Failed to write to mpsc channel
    #[error("Writing to mpsc channel: {0}")]
    Channel(#[from] mpsc::SendError),

    /// Failed to write to a WebSocket channel
    #[error("Writing to websocket: {0}")]
    WebSocket(#[from] Arc<WsError>),

    /// Failed to write to a WebSocket channel
    #[error("Parsing URL: {0}")]
    InvalidURL(#[from] ParseError),

    /// Unsupported Onchain URI scheme
    #[error("Unsupported URI scheme: {0:?}")]
    UriScheme(Option<String>),

    /// Stream closed unexpectedly
    #[error("Stream closed unexpectedly")]
    Closed,

    /// Error creating wallet
    #[error("Creating wallet: {0:?}")]
    Wallet(#[from] Arc<WalletError>),

    /// Error from cubist config (e.g., reading secrets)
    #[error("Secret read error: {0}")]
    ReadSecretError(String),
}

into_arc_variant!(FatalErr, WsError, WebSocket);
into_arc_variant!(FatalErr, WalletError, Wallet);

/// An error message for a JSON-RPC processing pipeline.
/// This is either a wrapper around `serde_json::Value`
/// or a fatal error that should kill the pipeline.
///
/// The intent is that elements in a processing pipeline
/// should generate descriptive error messages and wrap
/// them in JsonRpcErr.
#[derive(Clone, Debug, Error)]
pub enum JsonRpcErr {
    /// JSON-RPC error response
    #[error("JSON-RPC error: {0}")]
    Jrpc(#[from] JrpcError),

    /// A fatal error that kills the pipeline
    // NOTE: do not implement From<FatalErr>! Explicit, manual conversions only.
    #[error("Fatal error: {0}")]
    Fatal(FatalErr),
}

impl From<JsonParseError> for JsonRpcErr {
    fn from(other: JsonParseError) -> Self {
        parse_error(other.to_string())
    }
}

/// An error message for an HTTP Request/Response pipeline.
/// This is either a wrapper around a Result or a fatal error
/// that should kill the pipeline.
///
/// As with JsonRpcErr, the idea is that individual elements of
/// the pipeline should generate detailed error messages and then
/// pass them along the pipeline wrapped in HttpRrErr.
#[derive(Debug, Error)]
pub enum HttpRrErr {
    /// HTTP error Response
    #[error("HTTP error response: {0:?}")]
    Http(Response<String>),

    /// A fatal error that kills the pipeline
    // NOTE: do not implement From<FatalErr>! Explicit, manual conversions only.
    #[error("Fatal error: {0}")]
    Fatal(FatalErr),
}

impl From<JsonParseError> for HttpRrErr {
    fn from(other: JsonParseError) -> Self {
        response(StatusCode::BAD_REQUEST, other.to_string()).into()
    }
}

impl From<Response<String>> for HttpRrErr {
    fn from(other: Response<String>) -> Self {
        Self::Http(other)
    }
}

impl From<JsonRpcErr> for HttpRrErr {
    fn from(other: JsonRpcErr) -> Self {
        match other {
            JsonRpcErr::Fatal(f) => Self::Fatal(f),
            JsonRpcErr::Jrpc(v) => {
                if v.id.is_notification() {
                    Self::Http(response(StatusCode::NO_CONTENT, "".to_owned()))
                } else {
                    Self::Http(response(StatusCode::OK, json_string(&v.into())))
                }
            }
        }
    }
}
