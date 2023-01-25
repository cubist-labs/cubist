/*! Pass - connects to a specified target and relays messages */

use crate::{connector::connect, jrpc_client, FatalErr, JrpcRequest, Offchain, Pair};

use hyper::Uri;
use serde_json::Value;
use tokio::task::JoinHandle;

/// A very simple passthru pipeline. This is a full-fledged pipeline that
/// takes an `Offchain` and handles it entirely.
pub fn pass(off: Offchain, uri: Uri) -> JoinHandle<Result<(), FatalErr>> {
    tokio::spawn(async move {
        tracing::debug!("pass pipeline starting with URI {uri:?}");
        pass_pair(off.jrpc(), uri).await
    })
}

/// A passthru pipeline that operates on a `Pair`. This is used in the case
/// that you have a pipeline that first does some processing on an `Offchain`
/// and only then applies the pass functionality to it. See the `ava` pipeline
/// for an example of the use of this functionality.
pub async fn pass_pair(
    jrpc: impl Pair<Value, JrpcRequest> + Unpin + 'static,
    uri: Uri,
) -> Result<(), FatalErr> {
    let client = jrpc_client(uri, None).await?;
    let res = connect(client, jrpc).await;
    tracing::debug!("pass pipeline exiting with {res:?}");
    res
}
