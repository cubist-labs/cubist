/*! Dump mux - print what's received, send nothing */

use crate::{jrpc::no_response, FatalErr, Offchain};

use futures::{SinkExt, StreamExt};
use tokio::task::JoinHandle;

/// A very simple dumper pipeline that quits after 6 messages
pub fn dump(off: Offchain) -> JoinHandle<Result<(), FatalErr>> {
    tokio::spawn(async move {
        let (mut snd, mut rcv) = off.jrpc().split();

        let mut receiver = rcv.by_ref().take(6);
        while let Some(m) = receiver.next().await {
            tracing::debug!("dump: {:?}", m);
            let resp = snd
                .send(Err(no_response(Some("pipeline::dump".to_owned()))))
                .await;
            tracing::debug!("      result: {:?}", resp);
            resp?;
        }

        tracing::debug!("dump [got 6 messages, closing]");
        let res = snd.close().await;
        tracing::debug!("dump [result: {:?}]", res);
        res
    })
}
