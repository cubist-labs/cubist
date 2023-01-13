/*! passthru + echo server
 *
 *  Run a pass pipeline that connects to an echo pipeline
 */

use std::net::SocketAddr;

use cubist_proxy::{
    listen_all,
    pipeline::{echo, pass},
};
use futures::StreamExt;
use hyper::Uri;

#[tokio::main]
async fn main() {
    const ECHO_PORT: u16 = 8080;
    const WS_PASS_PORT: u16 = 8082;
    const HTTP_PASS_PORT: u16 = 8083;
    const HOST: [u8; 4] = [127, 0, 0, 1];

    tracing_subscriber::fmt::init();

    let ws_url = Uri::try_from(&format!("ws://localhost:{ECHO_PORT}")).unwrap();
    let ws_pass_fn = |c| pass(c, ws_url.clone());

    let http_uri = Uri::try_from(&format!("http://localhost:{ECHO_PORT}")).unwrap();
    let http_pass_fn = |c| pass(c, http_uri.clone());

    listen_all! {
        (echo, SocketAddr::from((HOST, ECHO_PORT)), echo),
        (ws_pass, SocketAddr::from((HOST, WS_PASS_PORT)), ws_pass_fn),
        (http_pass, SocketAddr::from((HOST, HTTP_PASS_PORT)), http_pass_fn)
    }
}
