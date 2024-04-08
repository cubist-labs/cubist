//! pipeline: take an Offchain and execute on it

use futures::future::{ready, Ready};
use tokio::task::JoinHandle;

use crate::Offchain;

mod ava;
mod dump;
mod echo;
mod eth;
mod pass;

pub use ava::ava;
pub use dump::dump;
pub use echo::echo;
pub use eth::{eth, eth_pair};
pub use pass::{pass, pass_pair};

/// Ignore the future that comes out of a call to f(), instead returning Ready<()>.
pub fn drop_join_handle<R>(
    f: impl Fn(Offchain) -> JoinHandle<R>,
) -> impl Fn(Offchain) -> Ready<()> {
    move |off| {
        drop(f(off));
        ready(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::jrpc::{no_response, Id, IdReq, Request};
    use crate::{jrpc_client, listen_on_addr, JsonRpcErr};

    use futures::{FutureExt, SinkExt, StreamExt};
    use rstest::rstest;
    use serde_json::{json, Value};
    use std::net::SocketAddr;

    #[rstest]
    #[case::http_http(false, false)]
    #[case::http_ws(false, true)]
    #[case::ws_http(true, false)]
    #[case::ws_ws(true, true)]
    #[tokio::test]
    async fn test_pass_echo(#[case] ws_echo: bool, #[case] ws_client: bool) {
        // OS chooses listen port
        let listen_addr = SocketAddr::from(([127, 0, 0, 1], 0));

        // echo pipeline
        let (echo_clients, echo_addr) = listen_on_addr(listen_addr);
        let call_echo = drop_join_handle(echo);
        let echo_uri: hyper::Uri = if ws_echo {
            format!("ws://127.0.0.1:{}", echo_addr.port())
        } else {
            format!("http://127.0.0.1:{}", echo_addr.port())
        }
        .try_into()
        .unwrap();

        // passthru pipeline that we'll connect to echo pipeline
        let (pass_clients, pass_addr) = listen_on_addr(listen_addr);
        let call_pass = drop_join_handle(move |c| pass(c, echo_uri.clone()));

        // handle clients for both pipelines
        tokio::spawn(async move {
            let echo_fut = echo_clients.for_each_concurrent(None, call_echo);
            let pass_fut = pass_clients.for_each_concurrent(None, call_pass);
            tokio::join!(echo_fut, pass_fut)
        });

        // client that connects to the pass pipeline
        let uri = if ws_client {
            format!("ws://127.0.0.1:{}", pass_addr.port())
        } else {
            format!("http://127.0.0.1:{}", pass_addr.port())
        }
        .parse()
        .unwrap();
        let mut client = jrpc_client(uri, None).await.unwrap();

        // sending an Err should send the same Err back to us
        let err = no_response(Some(Value::Null));
        client.send(Err(err)).await.unwrap();
        assert!(matches!(
            client.next().await,
            Some(Err(JsonRpcErr::Jrpc(_)))
        ));
        assert!(client.next().now_or_never().is_none());

        // send a non-Notification request, get back same request
        let req = Request::with_params(Id::from(4), "test_1", Some(json!([1, 2, 3])));
        client.send(Ok(req.clone())).await.unwrap();
        let resp = Request::try_from(client.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(req, resp);
        assert!(client.next().now_or_never().is_none());

        // send a Notification request
        let req = Request::with_params(IdReq::Notification, "test_2", Some(json!("asdf")));
        client.send(Ok(req)).await.unwrap();
        if !ws_client {
            // http client gets back "no-response" message
            if let JsonRpcErr::Jrpc(resp) = client.next().await.unwrap().unwrap_err() {
                assert_eq!(
                    (resp.id, resp.error.code),
                    (IdReq::Notification, (-32004).into())
                );
            } else {
                unreachable!("error was not Jrpc");
            }
        }
        assert!(client.next().now_or_never().is_none());
    }
}
