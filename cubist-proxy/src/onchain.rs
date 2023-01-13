// Copyright 2022 Riad S. Wahby <r@cubist.dev> and the Cubist developers
//
// This file is part of cubist-proxy.
//
// See LICENSE for licensing terms. This file may not be copied,
// modified, or distributed except according to those terms.

//! Onchain: a (String sink/stream) <-> (chainward network connection) adapter

use std::pin::Pin;

use futures::{future::ready, SinkExt, StreamExt, TryStreamExt};
use hyper::{
    body::to_bytes, client::HttpConnector, Body, Client as HyperClient, Error as HyperError,
    Request, Response, StatusCode, Uri,
};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{
    connector::{passthru_pair, ConcretePair},
    jrpc::{error, no_response},
    offchain::response,
    transformer::{errors_to_stream, json},
    FatalErr, HttpRrErr, JrpcRequest, JsonRpcErr, Pair,
};

/// Construct a JSON-RPC client from a ws:, wss:, http:, or https: URI
pub async fn jrpc_client(
    uri: Uri,
    pool: Option<ConnPool>,
) -> Result<impl Pair<JrpcRequest, Value> + Unpin, FatalErr> {
    let is_ws = match uri.scheme_str() {
        Some("http" | "https") => false,
        Some("ws" | "wss") => true,
        scheme => return Err(FatalErr::UriScheme(scheme.map(ToString::to_string))),
    };

    if is_ws {
        tracing::debug!("Spawning Websocket client for {:?}", uri);
        let (ws, _) = connect_async(uri).await?;
        Ok(Box::pin(ws_jrpc(ws)) as Pin<Box<dyn Pair<_, _>>>)
    } else {
        tracing::debug!("Spawning HTTP client for {:?}", uri);
        Ok(Box::pin(http_jrpc(http_client(pool), uri)) as Pin<Box<dyn Pair<_, _>>>)
    }
}

/// A connection pool for making HTTP or HTTPS requests
pub type ConnPool = HyperClient<HttpsConnector<HttpConnector>, Body>;

/// Construct a connection pool for use with http_client(), jrpc_client(), or rr_client()
pub fn conn_pool() -> ConnPool {
    // XXX(question) .enable_http2() too?
    let https_conn = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .build();
    HyperClient::builder().build(https_conn)
}

/// Construct an HTTP client as a sink/stream pair that converts
/// Result<Request<Body>, Error> into Result<Response<Body>, Error>
pub fn http_client(pool: Option<ConnPool>) -> impl Pair<Request<Body>, Response<Body>, HyperError> {
    // Cleverness courtesy of @djrenren
    //
    // Make a Connector that sends to itself; this exposes both a Sink and a Stream, s.t.
    // anything written to the Sink ends up at the Stream. Assume that this Sink/Stream
    // pair passes Result<Request, Error>.
    //
    // We can postprocess these Result<Request, Error> values with an async closure
    // via StreamExt::then; and in particular we can use an async closure that
    // calls HyperClient::request, which turns a Request into a Response. This gives
    // us a Sink<Result<Request, Error>> and a Stream<Output = Result<Response, Error>>,
    // which is what we wanted.
    let client = pool.unwrap_or_else(conn_pool);
    ConcretePair::pipe()
        .sink_err_into()
        .inspect(|req| tracing::trace!("Got request {req:?}"))
        .and_then(move |req| client.request(req))
        .inspect(|resp| tracing::trace!("Got response {resp:?}"))
}

/// Construct an HTTP client that takes Request/Response directly from the pipeline
pub fn rr_client(
    pool: Option<ConnPool>,
) -> impl Pair<Request<String>, Response<String>, HttpRrErr> {
    tracing::debug!("rr_client starting");
    let http = http_client(pool)
        .with(|req: Result<Request<String>, HttpRrErr>| match req {
            Err(_) => unreachable!(), // because of errors_to_stream
            Ok(req) => ready(Ok(Ok(req.map(Into::into)))),
        })
        .then(|resp: Result<Response<Body>, HyperError>| async {
            match resp {
                Err(e) => Err(e.to_string()),
                Ok(resp) => {
                    let (parts, body) = resp.into_parts();
                    match to_bytes(body).await {
                        Err(e) => Err(e.to_string()),
                        Ok(b) => match String::from_utf8(b.to_vec()) {
                            Ok(s) => Ok(Response::from_parts(parts, s)),
                            Err(e) => Err(e.to_string()),
                        },
                    }
                }
            }
            .map_err(|s| response(StatusCode::BAD_GATEWAY, s).into())
        });
    errors_to_stream(http)
}

// Convert the Request/Response values from the HTTP endpoint to JSON Values.
fn http_jrpc(
    mut http: impl Pair<Request<Body>, Response<Body>, HyperError> + Unpin + 'static,
    uri: Uri,
) -> impl Pair<JrpcRequest, Value> {
    let (theirs, mut ours) = passthru_pair::<JrpcRequest, Value, JsonRpcErr>();

    tokio::spawn(async move {
        while let Some(res_v) = ours.next().await {
            let val = res_v.expect("error_to_stream should have prevented this!");
            // get the ID of this JSON-RPC request
            let id = val.id.clone();
            // convert to an HTTP request
            let req = Request::post(&uri)
                .header("Content-Type", "application/json")
                .body(val.to_string().into())
                .expect("all generated Requests should be valid");

            // send the HTTP request
            tracing::trace!("HTTP request: {req:?}");
            if let Err(e) = http.send(Ok(req)).await {
                tracing::debug!("HTTP request error {e:?}");
                if let Err(e) = ours.send(Err(JsonRpcErr::Fatal(e))).await {
                    tracing::debug!("passing error downstream: {e:?}");
                }
                break;
            }

            // wait for the response and make appropriate error messages
            let resp = match http.next().await {
                Some(resp) => resp,
                None => {
                    // HTTP stream died
                    let snd = ours.send(Err(JsonRpcErr::Fatal(FatalErr::Closed))).await;
                    tracing::debug!("HTTP stream closed / {snd:?}");
                    break;
                }
            };
            let resp = match resp {
                Err(e) => Err(error(-32000, "making HTTP request", id, e.to_string())),
                Ok(resp) if resp.status() == StatusCode::NO_CONTENT => {
                    // 204 means we send nothing back
                    tracing::debug!("HTTP sent back 204 NO CONTENT");
                    Err(no_response(Some("onchain::http_jrpc".to_owned())))
                }
                Ok(resp) if resp.status() != StatusCode::OK => {
                    // otherwise, non-200 means an error
                    let err_code = -32000 + i64::from(resp.status().as_u16());
                    let err_msg = resp.status().canonical_reason().unwrap_or("");
                    Err(error(err_code, "HTTP status code", id, err_msg))
                }
                Ok(resp) => {
                    // 200 means we have a response to send
                    match to_bytes(resp.into_body()).await {
                        // return server error if we can't get the response body
                        Err(e) => Err(error(-32001, "HTTP response", id, e.to_string())),
                        Ok(b) => match String::from_utf8(b.to_vec()) {
                            // NOTE we don't use transformer::convert::json here because
                            // the error there is -32700, whereas we want to send back a
                            // more informative error message that includes the request-id.
                            Ok(s) => match serde_json::from_str(&s) {
                                Ok(j) => Ok(j),
                                Err(e) => Err(error(-32003, "HTTP to JSON", id, e.to_string())),
                            },
                            Err(e) => Err(error(-32002, "HTTP to UTF8", id, e.to_string())),
                        },
                    }
                }
            };

            tracing::trace!("HTTP response: {resp:?}");
            if let Err(e) = ours.send(resp).await {
                tracing::debug!("Error returning HTTP response: {e:?}");
                break;
            }
        }

        tracing::debug!("http_jrpc thread exiting");
    });

    errors_to_stream(theirs)
}

// Convert the Message values from the Websocket endpoint to Strings.
// The Websocket endpoint doesn't handle Err()s, so we short-circuit them here.
fn ws_jrpc(ws: WebSocketStream<MaybeTlsStream<TcpStream>>) -> impl Pair<JrpcRequest, Value> {
    let ws = ws
        // Stream of Result<Message, WsError> -> Result<String, Error>
        .filter_map(|m| async {
            match m {
                Err(e) => {
                    tracing::debug!("got error {e:?}");
                    Some(Err(JsonRpcErr::Fatal(e.into())))
                }
                Ok(Message::Text(t)) => {
                    tracing::trace!("got {t:?}");
                    Some(Ok(t))
                }
                Ok(Message::Close(ic)) => {
                    tracing::debug!("got close frame with message {ic:?}");
                    Some(Err(JsonRpcErr::Fatal(FatalErr::Closed)))
                }
                Ok(other) => {
                    tracing::trace!("ignoring non-Text message {other:?}");
                    None
                }
            }
        })
        .inspect(|resp| tracing::trace!("Got response {resp:?}"))
        // Sink of Result<String, Error> -> Sink of Message
        .with(|res_s: Result<String, JsonRpcErr>| async {
            tracing::trace!("Got request {res_s:?}");
            Ok::<_, FatalErr>(Message::Text(
                res_s.expect("error_to_stream should have prevented this!"),
            ))
        });

    // short-circuit errors, convert JrpcRequest/Value to/from Strings
    json(errors_to_stream(ws))
}
