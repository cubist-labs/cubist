// Copyright 2022 Riad S. Wahby <r@cubist.dev> and the Cubist developers
//
// This file is part of cubist-proxy.
//
// See LICENSE for licensing terms. This file may not be copied,
// modified, or distributed except according to those terms.

//! Avalanche request handling

use futures::{future::ready, SinkExt, StreamExt};
use hyper::{
    http::{uri::PathAndQuery, StatusCode},
    Response, Uri,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::{
    conn_pool,
    connector::{passthru, MpscPair},
    offchain::{json_string, response},
    pipeline::{eth_pair, pass_pair},
    rr_client,
    transformer::{canon_request, eth_creds::CredProxy},
    ConnPool, FatalErr, HttpRrErr, JsonRpcErr, Offchain,
};

// used inside ava()
type HandlerMap = HashMap<String, MpscPair<String, Result<Response<String>, HttpRrErr>>>;

/// Handle either HTTPS or WebSocket connection for Avalanche EVM chains
///
/// Given an Offchain, we determine how to handle it as follows:
///
/// - HTTP connection: we need to consider requests one by one because an HTTP Offchain
///   can produce requests to multiple paths. For each request, we examine the path and
///   dispatch accordingly:
///   - if we have creds for this chain-id, handle it using long-lived `eth_creds` instances.
///   - otherwise, just pass through the request to the server specified by `uri`
///
/// - Websocket connection to path /ext/bc/&lt;chain-id&gt;/ws
///   - if we have creds for this chain-id, handle it using `eth_creds`
///   - otherwise, just pass through the request to the server specified by `uri`
///
/// ## Arguments
///
/// - `off` is the newly arrived `Offchain`
///
/// - `uri` is the URI of the node endpoint. This `Uri`'s path and query are ignored,
///  since they are replaced with the proper path per-query. Note also that this Uri
///  should have an HTTP or HTTPS scheme; when a websocket is needed, the scheme will
///  be replaced with `ws` or `wss`, respectively (this always works because `wss` means
///  to make an HTTPS connection and then upgrade to websocket).
///
/// - `accs` is a map from chain-id to `Accounts`. For example, if you have credentials
///   for the C-chain and a chain with id "foobar", your map should contain keys "C"
///   and "foobar" with corresponding `Accounts` values.
pub fn ava(
    off: Offchain,
    uri: Uri,
    proxies: Arc<HashMap<String, Arc<CredProxy>>>,
) -> JoinHandle<Result<(), FatalErr>> {
    tokio::spawn(async move {
        // figure out if this is HTTP or WebSocket
        let mut rr = match off.rr() {
            // WebSocket
            Err((ws, p)) => {
                let uri = ws_uri(uri, p.uri.path().try_into().expect("valid uri"));
                match ws_chainid(p.uri.path()) {
                    // this is a request to a known chain's endpoint, connect it up
                    Some(id) if proxies.contains_key(id) => {
                        let proxy = proxies[id].clone();
                        tracing::debug!("eth (ava ws) pipeline starting for uri {uri:?}");
                        return eth_pair(Offchain::WebSocket(ws, p).jrpc(), uri, proxy, None).await;
                    }

                    // not a websocket request we can handle; just pass it onwards
                    _ => {
                        tracing::debug!("pass (ava ws) pipeline starting for uri {uri:?}");
                        return pass_pair(Offchain::WebSocket(ws, p).jrpc(), uri).await;
                    }
                }
            }

            // HTTP
            Ok(rr) => rr,
        };

        // Connection pool for all requests that come from this pipeline
        let pool = conn_pool();
        // passthru client for requests we can't handle
        let mut pass = rr_client(Some(pool.clone()));
        // handlers for all the chains for which we have credentials
        let mut handlers: HandlerMap = HashMap::new();

        // handle HTTP requests synchronously
        while let Some(req) = rr.next().await {
            // send errors back to the source
            let req = match req {
                Ok(req) => req,
                Err(e) => {
                    rr.send(Err(e)).await?;
                    continue;
                }
            };

            let (parts, body) = req.into_parts();
            match http_chainid(parts.uri.path()) {
                // request to a known chain's endpoint
                Some(id) if proxies.contains_key(id) => {
                    if !handlers.contains_key(id) {
                        tracing::debug!("starting ava HTTP handler for {id}");
                        // no handler created yet; start one up
                        let (theirs, ours) =
                            passthru::<Result<Response<String>, HttpRrErr>, String>();
                        let uri = canon_uri(uri.clone(), parts.uri.path_and_query());
                        let worker_fut = ava_http(theirs, uri, proxies[id].clone(), pool.clone());
                        tokio::spawn(async move { worker_fut.await });
                        handlers.insert(id.to_owned(), ours);
                    }
                    // unwrap OK: if handlers didn't contain the key before, it does now
                    let handler = handlers.get_mut(id).unwrap();
                    // send request, receive response
                    handler.send(body).await?;
                    match handler.next().await {
                        None => break,
                        Some(r) => rr.send(r).await?,
                    }
                }

                // not a request we can handle; just pass it onwards
                _ => {
                    pass.send(Ok(canon_request(parts, body, uri.clone())))
                        .await?;
                    match pass.next().await {
                        None => break,
                        Some(r) => rr.send(r).await?,
                    }
                }
            }
        }
        tracing::debug!("ava http pipeline exiting normally (stream closed)");
        Ok(())
    })
}

/// Handle HTTP connection for Avalanche
async fn ava_http(
    ava: MpscPair<Result<Response<String>, HttpRrErr>, String>,
    uri: Uri,
    proxy: Arc<CredProxy>,
    pool: ConnPool,
) -> Result<(), FatalErr> {
    tracing::debug!("eth (ava http) pipeline starting with URI {uri:?}");

    // In principle, this Pair asynchronously processes requests and responses.
    // The `ava` function, however, always sends a request and then waits for
    // the corresponding response when interacting with this function, because
    // this adapter is only used with HTTP (not websocket) clients.
    let ava = ava
        .map(|body: String| serde_json::from_str(&body).map_err(Into::into))
        .with(|r: Result<Value, JsonRpcErr>| {
            ready(Ok(r
                .map_err(HttpRrErr::from)
                .map(|v| response(StatusCode::OK, json_string(&v)))))
        });

    eth_pair(ava, uri, proxy, Some(pool)).await
}

/// Match the Avalanche blockchain endpoint prefix.
fn ava_prefix(path: &str) -> Option<&str> {
    path.strip_prefix("/ext/bc/")
}

/// Try to match an Avalanche websocket endpoint name, returning chain-id if successful.
fn ws_chainid(path: &str) -> Option<&str> {
    ava_prefix(path).and_then(|p| p.strip_suffix("/ws"))
}

/// Try to match an Avalanche HTTP endpoint name, returning chain-id if successful.
fn http_chainid(path: &str) -> Option<&str> {
    ava_prefix(path).and_then(|p| p.strip_suffix("/rpc"))
}

/// Generate a canonical URI by replacing `uri`'s path and query with what was
/// specified in `pq`.
fn canon_uri(uri: Uri, pq: Option<&PathAndQuery>) -> Uri {
    let mut parts = uri.into_parts();
    parts.path_and_query = pq.cloned();
    Uri::from_parts(parts).expect("valid parts")
}

/// Generate a canonical websocket URI as in `canon_uri`, and also change the scheme
/// to "ws" or "wss" depending on `uri`'s scheme ("http" -> "ws", "https" -> "wss").
fn ws_uri(uri: Uri, pq: PathAndQuery) -> Uri {
    let ws_scheme = match uri.scheme_str() {
        Some("https" | "wss") => "wss",
        _ => "ws",
    };
    let mut parts = uri.into_parts();
    parts.path_and_query = Some(pq);
    parts.scheme = Some(ws_scheme.try_into().expect("valid scheme"));
    Uri::from_parts(parts).expect("valid parts")
}

#[cfg(test)]
mod test {
    use super::{ava, canon_uri, http_chainid, ws_chainid, ws_uri};
    use crate::{
        jrpc_client, listen_on_addr,
        pipeline::{drop_join_handle, echo},
        transformer::eth_creds::{CredProxy, EthProxyConfig},
        JrpcRequest,
    };

    use cubist_config::network::{CredConfig, PrivateKeyConfig};
    use futures::{FutureExt, SinkExt, StreamExt};
    use hyper::Uri;
    use rstest::rstest;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;

    #[test]
    fn test_ws_uri() {
        let uri: Uri = "http://127.0.0.1:1234".try_into().unwrap();
        let uri = ws_uri(uri, "".try_into().unwrap());
        assert_eq!(uri.scheme_str(), Some("ws"));
        assert_eq!(uri.path(), "/");

        let uri: Uri = "https://127.0.0.1:2345".try_into().unwrap();
        let uri = ws_uri(uri, "/asdf/qwer?zxcv=uiop".try_into().unwrap());
        assert_eq!(uri.scheme_str(), Some("wss"));
        assert_eq!(
            uri.path_and_query(),
            Some(&"/asdf/qwer?zxcv=uiop".try_into().unwrap())
        );
    }

    #[test]
    fn test_ws_http_chainid() {
        assert_eq!(ws_chainid("/ext/bc"), None);
        assert_eq!(ws_chainid("/ext/bc/ws"), None);
        assert_eq!(ws_chainid("/ext/bc//ws"), Some(""));
        assert_eq!(ws_chainid("/ext/bc//rpc"), None);
        assert_eq!(
            ws_chainid("/ext/bc/asdf+qwer/zxcv/ws"),
            Some("asdf+qwer/zxcv")
        );
        assert_eq!(ws_chainid("/ext/bc/asdf+qwer/zxcv/ws/"), None);

        assert_eq!(http_chainid("/ext/bc"), None);
        assert_eq!(http_chainid("/ext/bc/rpc"), None);
        assert_eq!(http_chainid("/ext/bc//rpc"), Some(""));
        assert_eq!(http_chainid("/ext/bc//ws"), None);
        assert_eq!(
            http_chainid("/ext/bc/asdf+qwer/zxcv/rpc"),
            Some("asdf+qwer/zxcv")
        );
        assert_eq!(http_chainid("/ext/bc/asdf+qwer/zxcv/rpc/"), None);
    }

    #[test]
    fn test_canon_uri() {
        let uri: Uri = "https://cubist.net:999///".try_into().unwrap();
        assert_eq!(
            canon_uri(uri.clone(), Some(&"/".try_into().unwrap())).path(),
            "/"
        );

        let uri2 = canon_uri(uri, Some(&"/asdf?qwer".try_into().unwrap()));
        assert_eq!(uri2.path(), "/asdf");
        assert_eq!(format!("{uri2}"), "https://cubist.net:999/asdf?qwer");
    }

    #[rstest]
    #[case::http_pass_c(true, true, "C")]
    #[case::http_pass_d(true, true, "D")]
    #[case::http_ava_c(true, false, "C")]
    #[case::http_ava_d(true, false, "D")]
    #[case::ws_pass_c(false, true, "C")]
    #[case::ws_pass_d(false, true, "D")]
    #[case::ws_ava_c(false, false, "C")]
    #[case::ws_ava_d(false, false, "D")]
    #[tokio::test]
    async fn test_ava_e2e(#[case] is_http: bool, #[case] is_pass: bool, #[case] chain_id: &str) {
        let listen_addr = SocketAddr::from(([127, 0, 0, 1], 0));

        // test pipeline endpoint
        let (mut clients, ws_addr) = listen_on_addr(listen_addr);

        // set up the echo server that pass_ws connects us to
        let (echo_clients, echo_addr) = listen_on_addr(listen_addr);
        let echo_uri: Uri = format!("http://127.0.0.1:{}", echo_addr.port())
            .try_into()
            .unwrap();
        tokio::spawn(async move {
            echo_clients
                .for_each_concurrent(None, drop_join_handle(echo))
                .await
        });

        // listen for a connection and start an ava signer with no creds
        assert!(clients.next().now_or_never().is_none());
        tokio::spawn(async move {
            let accs = if is_pass {
                Default::default()
            } else {
                let config = EthProxyConfig {
                    onchain_uri: None,
                    chain_id: 1048576,
                    creds: vec![CredConfig::PrivateKey(PrivateKeyConfig {
                        hex: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                            .to_string()
                            .into(),
                    })],
                };
                let proxy = Arc::new(CredProxy::from_cfg(&config).unwrap());
                let mut proxies = HashMap::new();
                proxies.insert("C".to_owned(), proxy);
                Arc::new(proxies)
            };
            clients
                .for_each_concurrent(
                    None,
                    drop_join_handle(move |o| ava(o, echo_uri.clone(), accs.clone())),
                )
                .await
        });

        let (sc, ex) = if is_http {
            ("http", "rpc")
        } else {
            ("ws", "ws")
        };
        let client_uri: Uri =
            format!("{sc}://127.0.0.1:{}/ext/bc/{chain_id}/{ex}", ws_addr.port(),)
                .try_into()
                .unwrap();

        // this request should get echoed back to us
        let mut client = jrpc_client(client_uri, None).await.unwrap();
        client
            .send(Ok(JrpcRequest::new(999, "asdf")))
            .await
            .unwrap();
        let resp = client.next().await.unwrap().unwrap();
        assert!(client.next().now_or_never().is_none());
        assert_eq!(resp["id"], 999);
        assert_eq!(resp["method"], "asdf");

        // this is a request that eth_creds handles
        client
            .send(Ok(JrpcRequest::new(998, "eth_accounts")))
            .await
            .unwrap();
        let resp = client.next().await.unwrap().unwrap();
        assert!(client.next().now_or_never().is_none());
        assert_eq!(resp["id"], 998);
        if is_pass || chain_id != "C" {
            assert_eq!(resp["method"], "eth_accounts");
        } else {
            const FIRST_ACCOUNT: &str = "0xc96aaa54e2d44c299564da76e1cd3184a2386b8d";
            assert_eq!(resp["result"][0], FIRST_ACCOUNT);
        }
    }
}
