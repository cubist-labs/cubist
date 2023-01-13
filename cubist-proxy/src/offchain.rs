// Copyright 2022 John Renner <j@cubist.dev> and the Cubist developers
//
// This file is part of cubist-proxy.
//
// See LICENSE for licensing terms. This file may not be copied,
// modified, or distributed except according to those terms.

//! Contains code for interacting with off-chain applications
//!
//! Typically these are abstractions that turn protocols into streams
use std::{convert::Infallible, fmt::Debug, net::SocketAddr, pin::Pin, sync::Arc};

use futures::{
    channel::mpsc, future::ready, lock::Mutex, Future, FutureExt, SinkExt, Stream, StreamExt,
};
use hyper::{
    body::to_bytes,
    http::{header::CONTENT_LENGTH, request::Parts, Method, StatusCode},
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    upgrade::Upgraded,
    Body, Request, Response, Server,
};
use hyper_tungstenite::{
    is_upgrade_request,
    tungstenite::{self, Message},
    upgrade, WebSocketStream,
};
use serde_json::Value;
use thiserror::Error;

use crate::{
    connector::{passthru, MpscPair},
    jrpc::Error as JrpcError,
    transformer::{errors_to_sink, json},
    FatalErr, HttpRrErr, JrpcRequest, JsonRpcErr, Pair,
};

/// Listens on a tcp port and produces `Offchain`s.
///
/// This function returns a stream of Offchains and a SocketAddr representing
/// the bound address. This is useful when passing a SocketAddr with port=0
/// as an argument, which asks the OS to bind any available port.
pub fn listen_on_addr(socket_addr: SocketAddr) -> (impl Stream<Item = Offchain>, SocketAddr) {
    let (client_w, client_o) = mpsc::channel(0);

    let make_service = make_service_fn(move |_: &AddrStream| {
        let mut client_w = client_w.clone();
        let (http_prod, http_cons) = passthru();
        let http_client = Offchain::Http(http_cons);
        tracing::debug!("HEY CLIENT");
        async move {
            client_w.send(http_client).await?;

            // Hyper service_fn is built to support pipelining which means the function could
            // be invoked twice at the same time. This shouldn't happen, but we need to use an
            // Arc<Mutex<T>> to keep it happy. We could possibly work around this if we implement
            // our own Service struct.
            //
            // XXX(question) implement Service rather than using service_fn?
            let (http_sender, http_receiver) = http_prod.into_parts();
            let http_receiver = Arc::new(Mutex::new(http_receiver));

            let service = service_fn(move |req: Request<Body>| {
                let http_receiver = Arc::clone(&http_receiver);
                let mut http_sender = http_sender.clone();
                let mut client_w = client_w.clone();

                async move {
                    if is_upgrade_request(&req) {
                        tracing::debug!("listen_on: got upgrade request, handling");
                        http_sender.close_channel();
                        let mut req = req;
                        let (resp, ws) = match upgrade(&mut req, None) {
                            Ok(rw) => rw,
                            Err(e) => {
                                return Ok(response(
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    format!("Upgrade failed: {e:?}"),
                                ))
                            }
                        };
                        tokio::spawn(async move {
                            let stream = ws.await?;
                            let (parts, _) = req.into_parts();
                            client_w
                                .send(Offchain::WebSocket(Box::new(stream), Box::new(parts)))
                                .await?;
                            Ok::<_, FatalErr>(())
                        });
                        return Ok(resp);
                    }
                    tracing::debug!("listen_on: not an upgrade request, handling");

                    // Lock the receive pipeline before sending
                    // This ensures that pipelined requests (which we should never get!)
                    // don't interfere with one another. Otherwise, req1 could send
                    // before req2 and then req2 could grab the lock before req1, in
                    // which caswe we'd respond to requests in the wrong order.
                    let mut rcv = http_receiver.lock().await;

                    let resp = if let Err(e) = http_sender.send(req).await {
                        response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("error passing request to pipeline: {e:?}"),
                        )
                    } else {
                        match rcv.next().await {
                            Some(resp) => resp,
                            None => response(StatusCode::INTERNAL_SERVER_ERROR, "Shutting down"),
                        }
                    };

                    Ok::<_, Infallible>(resp)
                }
            });

            Ok::<_, FatalErr>(service)
        }
    });

    let s = Server::bind(&socket_addr).serve(make_service);
    let localaddr = s.local_addr();

    tokio::spawn(async { s.await });

    (client_o, localaddr)
}

/// Listens on a tcp port and produces `Offchain`s
pub fn listen_on(socket_addr: SocketAddr) -> impl Stream<Item = Offchain> {
    listen_on_addr(socket_addr).0
}

/// `Offchain`s represent a unique tcp connection from an offchain client.
/// They're emitted by the `http` function
pub enum Offchain {
    /// A tcp connection operating over http
    Http(HttpPairOff),
    /// A tcp connection operating over websockets
    /// plus the HTTP request that triggered the upgrade
    /// for use in path-based routing.
    WebSocket(Box<WebSocketStream<Upgraded>>, Box<Parts>),
}

impl Offchain {
    /// Implements JSON RPC on top of the underlying client
    ///
    /// Note: this function does not parse the JSON for you, but it handles all
    ///       protocol level issues like HTTP headers and websocket types
    pub fn jrpc(self) -> impl Pair<Value, JrpcRequest> + Unpin {
        let p = match self {
            Self::Http(http) => {
                Box::pin(http_jrpc(http)) as Pin<Box<dyn Pair<String, String, OffchainErr>>>
            }
            Self::WebSocket(ws, _) => {
                Box::pin(ws_jrpc(*ws)) as Pin<Box<dyn Pair<String, String, OffchainErr>>>
            }
        };

        // Short-circuit errors back to the client.
        //
        // Explicitly call JrpcRequest::try_from rather than letting serde_json
        // generate a parse error. This lets us generate better err messages.
        json(errors_to_sink(p)).map(|v: Result<Value, _>| v.and_then(JrpcRequest::try_from))
    }

    /// Get a Pair that returns HTTP requests and responses, if possible.
    /// If this Offchain is a websocket, simply returns Self.
    pub fn rr(
        self,
    ) -> Result<
        impl Pair<Response<String>, Request<String>, HttpRrErr> + Unpin,
        (Box<WebSocketStream<Upgraded>>, Box<Parts>),
    > {
        match self {
            Self::WebSocket(ws, p) => Err((ws, p)),
            Self::Http(http) => Ok(errors_to_sink(http_rr(http))),
        }
    }
}

type HttpPairOff = MpscPair<Response<Body>, Request<Body>>;

fn http_with<Ti, FutI, To, FutO>(
    s: HttpPairOff,
    fi: impl Fn(Ti) -> FutI + Send,
    fo: impl Fn(Request<Body>) -> FutO + Send,
) -> impl Pair<Ti, To, OffchainErr>
where
    Ti: Debug,
    FutI: Future<Output = Result<Response<Body>, FatalErr>> + Send,
    To: Send,
    FutO: Future<Output = Result<To, OffchainErr>> + Send,
{
    let with_fut = move |s: Result<Ti, OffchainErr>| match s {
        Ok(t) => fi(t).left_future(),
        Err(e) => ready(e.try_into()).right_future(),
    };

    let then_fut = move |req: Request<Body>| match check_req(req) {
        Ok(req) => fo(req).left_future(),
        Err(e) => ready(Err(e)).right_future(),
    };

    s.with(with_fut).then(then_fut)
}

/// examine HTTP headers to make sure this is a legal request
fn check_req(req: Request<Body>) -> Result<Request<Body>, OffchainErr> {
    if req.method() != Method::POST {
        tracing::debug!("Method is not POST");
        return Err(OffchainErr::Method(req.into_parts().0.method));
    }

    // XXX(adversarial-client) enforce max content-length
    // Could send back 413 Payload Too Large and close connection
    if !req.headers().contains_key(CONTENT_LENGTH) {
        tracing::debug!("No Content-Length header found");
        return Err(OffchainErr::NoLength);
    }

    /*
    // strict header checking --- disabled for now
    {
    use hyper::http::header::*;
    match parts.headers.get(CONTENT_TYPE) {
    Some(r) if r == "application/json-rpc" => (),
    Some(r) => tracing::warn!(
    "Expected Content-type application/json-rpc, got {:?}",
    r
    ),
    None => tracing::warn!("Got request with no content-type header"),
    };
    match parts.headers.get(ACCEPT) {
    Some(r) if r == "application/json-rpc" => (),
    Some(r) => {
    tracing::warn!("Expected Accept application/json-rpc, got {:?}", r)
    }
    None => tracing::warn!("Got request with no accept header"),
    };
    }
    */

    Ok(req)
}

// HTTP request/response pair
// XXX(adversarial-client) mitigate slowloris with timeout?
fn http_rr(pair: HttpPairOff) -> impl Pair<Response<String>, Request<String>, OffchainErr> {
    http_with(
        pair,
        |resp: Response<String>| ready(Ok(resp.map(From::from))),
        |req: Request<Body>| async move {
            let (parts, body) = req.into_parts();
            // Pull in entire Request before sending down the pipe
            tracing::debug!("http_rr getting bytes");
            let bytes = to_bytes(body).await;
            tracing::trace!("http_rr: {bytes:?}");
            match bytes {
                Err(e) => Err(e.into()), // hyper::Error
                Ok(b) => String::from_utf8(b.to_vec())
                    .map_err(From::from)
                    .map(|s| Request::from_parts(parts, s)),
            }
        },
    )
}

// HTTP jrpc pair
// XXX(adversarial-client) mitigate slowloris with timeout?
fn http_jrpc(pair: HttpPairOff) -> impl Pair<String, String, OffchainErr> {
    http_with(
        pair,
        |s| ready(Ok(response(StatusCode::OK, s))),
        |req| async move {
            tracing::debug!("http_jrpc getting bytes");
            let bytes = to_bytes(req.into_body()).await;
            tracing::trace!("http_jrpc: {bytes:?}");
            match bytes {
                // this request failed, but don't kill the whole pipeline
                Err(e) => Err(e.into()), // hyper::Error
                Ok(b) => String::from_utf8(b.to_vec()).map_err(From::from),
            }
        },
    )
}

fn ws_jrpc(ws: WebSocketStream<Upgraded>) -> impl Pair<String, String, OffchainErr> {
    ws.with(|s: Result<String, OffchainErr>| async move {
        match s {
            Err(e) => e.try_into(),
            Ok(s) => Ok(Message::Text(s)),
        }
    })
    .filter_map(|m: Result<Message, tungstenite::Error>| async {
        match m {
            Err(e) => {
                tracing::debug!("got error {e:?}");
                Some(Err(e.into()))
            }
            Ok(Message::Text(t)) => {
                tracing::trace!("got {t:?}");
                Some(Ok(t))
            }
            Ok(Message::Close(ic)) => {
                tracing::debug!("got close frame with message {ic:?}");
                Some(Err(OffchainErr::Fatal(FatalErr::Closed)))
            }
            Ok(other) => {
                tracing::trace!("ignoring non-Text message {other:?}");
                None
            }
        }
    })
}

/// An error in the Offchain logic that accepts connections from clients.
#[derive(Debug, Error)]
enum OffchainErr {
    /// Http transport error
    #[error("Http transport error: {0}")]
    Hyper(#[from] hyper::Error),

    /// Conversion to String failed
    #[error("Conversion String failed: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),

    /// Got a non-POST request
    #[error("HTTP POST expected, got {0}")]
    Method(Method),

    /// HTTP request has no Content-Length header
    #[error("HTTP request has no Content-Length header")]
    NoLength,

    /// JSON-RPC error response
    #[error("JSON-RPC error: {0}")]
    Jrpc(Value),

    /// An HTTP error that's already formatted
    #[error("HTTP error response: {0:?}")]
    Http(Response<Body>),

    /// A fatal error
    #[error("Fatal error: {0}")]
    Fatal(FatalErr),
}

impl From<tokio_tungstenite::tungstenite::Error> for OffchainErr {
    fn from(other: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::Fatal(other.into())
    }
}

impl TryFrom<OffchainErr> for Response<Body> {
    type Error = FatalErr;

    fn try_from(other: OffchainErr) -> Result<Self, Self::Error> {
        use OffchainErr::*;
        let (code, body) = match other {
            Hyper(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:?}")),
            FromUtf8(e) => (StatusCode::BAD_REQUEST, format!("{e:?}")),
            Method(m) => (StatusCode::METHOD_NOT_ALLOWED, format!("{m:?}")),
            NoLength => (StatusCode::LENGTH_REQUIRED, "".into()),
            Jrpc(v) => value_to_http(&v),
            Http(resp) => return Ok(resp),
            Fatal(e) => return Err(e),
        };
        Ok(response(code, body))
    }
}

impl TryFrom<OffchainErr> for Message {
    type Error = FatalErr;

    fn try_from(other: OffchainErr) -> Result<Self, Self::Error> {
        use OffchainErr::*;
        match other {
            Hyper(_) => unreachable!(),    // only for HTTP
            FromUtf8(_) => unreachable!(), // only for HTTP
            Method(_) => unreachable!(),   // only for HTTP
            NoLength => unreachable!(),    // only for HTTP
            Jrpc(v) => Ok(value_to_ws(&v)),
            Http(_) => unreachable!(), // only for HTTP
            Fatal(e) => Err(e),
        }
    }
}

// OffchainErr representation of an error that should not be sent on the wire.
const JRPC_NO_CONTENT: Value = Value::Null;

fn jrpc_err_to_value(j: JrpcError) -> Value {
    if j.id.is_notification() {
        JRPC_NO_CONTENT
    } else {
        j.into()
    }
}

impl From<JsonRpcErr> for OffchainErr {
    fn from(other: JsonRpcErr) -> Self {
        match other {
            JsonRpcErr::Fatal(s) => Self::Fatal(s),
            JsonRpcErr::Jrpc(v) => Self::Jrpc(jrpc_err_to_value(v)),
        }
    }
}

impl From<HttpRrErr> for OffchainErr {
    fn from(other: HttpRrErr) -> Self {
        match other {
            HttpRrErr::Fatal(s) => Self::Fatal(s),
            HttpRrErr::Http(r) => Self::Http(r.map(Body::from)),
        }
    }
}

pub(crate) fn json_string(v: &Value) -> String {
    serde_json::to_string(v).expect("serde_json::Value string conversion should not fail")
}

// convert a JSON-RPC error message to a WebSocket Message
fn value_to_ws(v: &Value) -> Message {
    if v == &JRPC_NO_CONTENT {
        // websocket no-op
        Message::Ping(vec![204])
    } else {
        Message::Text(json_string(v))
    }
}

// convert a JSON-RPC error message to an HTTP response
fn value_to_http(v: &Value) -> (StatusCode, String) {
    if v == &JRPC_NO_CONTENT {
        (StatusCode::NO_CONTENT, "".to_owned())
    } else {
        (StatusCode::OK, json_string(v))
    }
}

/// Generate an HTTP Response from a status code and body
pub(crate) fn response<B, S: Into<B>>(code: StatusCode, body: S) -> Response<B> {
    Response::builder()
        .status(code)
        .body(body.into())
        .expect("all generated Responses should be valid")
}

#[cfg(test)]
mod test {
    use super::{listen_on_addr, Offchain};
    use crate::{
        onchain::{jrpc_client, rr_client},
        JrpcRequest,
    };

    use futures::{FutureExt, SinkExt, StreamExt};
    use hyper::{http::uri::Parts, Request, Response, Uri};
    use rand::{distributions::Alphanumeric, thread_rng, Rng};
    use std::net::SocketAddr;

    #[tokio::test]
    async fn test_rr() {
        let listen_addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let mut rng = thread_rng();
        let status: u16 = rng.gen_range(200..1000);
        let body = (0..32)
            .map(|_| rng.sample(Alphanumeric) as char)
            .collect::<String>();
        let path = (0..32)
            .map(|_| rng.sample(Alphanumeric) as char)
            .collect::<String>();
        let query = (0..32)
            .map(|_| rng.sample(Alphanumeric) as char)
            .collect::<String>();

        let (mut rr_clients, rr_addr) = listen_on_addr(listen_addr);
        let rr_uri: Uri = format!("http://127.0.0.1:{}/{path}?{query}", rr_addr.port())
            .try_into()
            .unwrap();

        // no clients yet!
        assert!(rr_clients.next().now_or_never().is_none());
        // wait for client to connect and echo request back
        tokio::spawn(async move {
            let mut server = rr_clients
                .next()
                .await
                .unwrap()
                .rr()
                .map_err(|_| ())
                .unwrap();
            let req = server.next().await.unwrap().unwrap();
            let (parts, body) = req.into_parts();
            let Parts {
                scheme,
                authority,
                path_and_query,
                ..
            } = parts.uri.into_parts();
            let path_and_query = path_and_query.expect("should have path and query");
            assert_eq!(scheme, None);
            assert_eq!(authority, None);
            assert_eq!(path_and_query.query(), Some(query.as_str()));
            assert_eq!(&path_and_query.path()[1..], path.as_str());

            let resp = Response::builder()
                .status(status)
                .body(body)
                .expect("should be ok");
            server.send(Ok(resp)).await.unwrap();
        });

        // connect
        let mut client = rr_client(None);
        let req = Request::post(&rr_uri)
            .header("Content-Type", "application/json")
            .body(body.clone())
            .expect("request should be valid");
        client.send(Ok(req)).await.unwrap();

        let resp = client.next().await.unwrap().unwrap();
        assert_eq!(resp.status(), status);
        assert_eq!(body, resp.into_body());
    }

    #[tokio::test]
    async fn test_rr_ws() {
        let listen_addr = SocketAddr::from(([127, 0, 0, 1], 0));

        let (mut rr_clients, rr_addr) = listen_on_addr(listen_addr);
        let rr_uri: Uri = format!("ws://127.0.0.1:{}", rr_addr.port())
            .try_into()
            .unwrap();

        // no clients yet!
        assert!(rr_clients.next().now_or_never().is_none());
        // wait for client to connect and echo request back
        tokio::spawn(async move {
            rr_clients.next().await.unwrap(); // we get an HTTP client before the websocket client
            let mut server = rr_clients
                .next()
                .await
                .unwrap()
                .rr()
                .map(|_| ())
                .map_err(|(ws, p)| Offchain::WebSocket(ws, p))
                .unwrap_err()
                .jrpc();
            let req = server.next().await.unwrap().unwrap();
            assert!(rr_clients.next().now_or_never().is_none());
            server.send(Ok(req.into())).await.unwrap();
        });

        // connect
        let mut client = jrpc_client(rr_uri, None).await.unwrap();
        client
            .send(Ok(JrpcRequest::new(999, "asdf")))
            .await
            .unwrap();

        let resp = client.next().await.unwrap().unwrap();
        assert_eq!(resp["id"], 999);
        assert_eq!(resp["method"], "asdf");
        assert!(client.next().now_or_never().is_none());
    }
}
