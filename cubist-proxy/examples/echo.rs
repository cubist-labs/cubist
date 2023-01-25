/*! Echo server */

use std::net::SocketAddr;

use cubist_proxy::{
    listen_on,
    pipeline::{drop_join_handle, echo},
};
use futures::StreamExt;

#[tokio::main]
async fn main() {
    const PORT: u16 = 8080;

    tracing_subscriber::fmt::init();

    // Listen for clients
    let clients = listen_on(SocketAddr::from(([127, 0, 0, 1], PORT)));

    eprintln!("Listening on port {PORT}");

    // Loop as long as we have clients
    clients
        .for_each_concurrent(None, drop_join_handle(echo))
        .await
}
