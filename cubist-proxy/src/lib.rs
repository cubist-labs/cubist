// Copyright 2022 Riad S. Wahby <r@cubist.dev> and the Cubist developers
//
// This file is part of cubist-proxy.
//
// See LICENSE for licensing terms. This file may not be copied,
// modified, or distributed except according to those terms.
#![warn(
    missing_docs,
    nonstandard_style,
    rust_2021_compatibility,
    rust_2018_idioms,
    clippy::unnested_or_patterns,
    clippy::redundant_closure_for_method_calls
)]
#![doc(html_no_source)]

//! rpcmux: framework for muxing TCP connections

pub mod connector;
mod error;
mod jrpc;
mod offchain;
mod onchain;
pub mod pipeline;
pub mod transformer;

pub use error::{FatalErr, HttpRrErr, JsonRpcErr};
pub use jrpc::Request as JrpcRequest;
pub use offchain::{listen_on, listen_on_addr, Offchain};
pub use onchain::{conn_pool, jrpc_client, rr_client, ConnPool};

use futures::{Sink, Stream};

/// A Sink/Stream pair in an RPC muxing pipeline
pub trait Pair<S, R = S, E = JsonRpcErr>:
    Stream<Item = Result<R, E>> + Sink<Result<S, E>, Error = FatalErr> + Send
{
}

impl<P, S, R, E> Pair<S, R, E> for P where
    P: Stream<Item = Result<R, E>> + Sink<Result<S, E>, Error = FatalErr> + Send
{
}

/// Set up one or more listeners and handle all of them in a loop
#[macro_export]
macro_rules! listen_all {
    { $(($id: ident, $port: expr, $call: expr)),+ } => {
        paste::paste! {
            $(
                let mut [<$id _clients>] = $crate::listen_on($port);
                tracing::info!("Listening on port {} for pipeline {}", $port, stringify!($call));
            )+

            loop {
                tokio::select! {
                    $(
                        [<$id _client>] = [<$id _clients>].next() => {
                            match [<$id _client>] {
                                None => {
                                    tracing::debug!("{} exited", stringify!($id));
                                    break
                                }
                                Some(c) => $call(c),
                            };
                        }
                    )+
                }
            }
        }
    }
}
