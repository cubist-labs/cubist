//! Ethereum request handling

use std::sync::Arc;

use crate::transformer::{debug, eth_creds::CredProxy};
use hyper::Uri;
use serde_json::Value;
use tokio::task::JoinHandle;

use crate::{connector::connect, jrpc_client, ConnPool, FatalErr, JrpcRequest, Offchain, Pair};

/// Intercepts Ethereum requests, answers all account and sign requests, and forwards
/// the rest to the endpoint specified by `uri`.
///
/// This is a full-fledged pipeline that takes an `Offchain` and handles it entirely.
pub fn eth(off: Offchain, uri: Uri, proxy: Arc<CredProxy>) -> JoinHandle<Result<(), FatalErr>> {
    tokio::spawn(async move {
        tracing::debug!("eth pipeline starting with URI {uri:?}");
        eth_pair(off.jrpc(), uri, proxy, None).await
    })
}

/// Intercepts Ethereum requests as in the case of `eth`, except that it operates on a `Pair`.
/// This is useful when you have a pipeline that needs to process the `Offchain` before applying
/// the `eth` functionality to it. See the `ava` pipeline for an example.
pub async fn eth_pair(
    jrpc: impl Pair<Value, JrpcRequest> + Unpin + 'static,
    uri: Uri,
    proxy: Arc<CredProxy>,
    pool: Option<ConnPool>,
) -> Result<(), FatalErr> {
    let jrpc = debug("offchain", jrpc);
    let eth = debug("eth_creds", proxy.wrap(jrpc));
    let client = debug("onchain", jrpc_client(uri, pool).await?);
    let res = connect(client, eth).await;
    tracing::debug!("eth pipeline exiting with {res:?}");
    res
}
