/*! Echo mux - loops back what's received */

use crate::{FatalErr, Offchain};
use futures::{SinkExt, StreamExt};
use tokio::task::JoinHandle;

/// A very simple echo pipeline
pub fn echo(off: Offchain) -> JoinHandle<Result<(), FatalErr>> {
    tokio::spawn(async move {
        let mut jrpc = off.jrpc();
        while let Some(req) = jrpc.next().await {
            tracing::debug!("{:?}", req);

            if let Err(e) = jrpc.send(req.map(Into::into)).await {
                tracing::warn!("echo pipeline exiting with error {e:?}");
                return Err(e);
            }
        }
        tracing::debug!("echo pipeline exiting normally");
        Ok(())
    })
}
