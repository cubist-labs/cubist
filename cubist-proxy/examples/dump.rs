/*! Dump server */

use std::net::SocketAddr;

use cubist_proxy::{
    listen_on,
    pipeline::{drop_join_handle, dump},
};
use futures::StreamExt;

#[tokio::main]
async fn main() {
    const PORT: u16 = 8081;

    tracing_subscriber::fmt::init();

    let clients = listen_on(SocketAddr::from(([127, 0, 0, 1], PORT)));
    eprintln!("Listening on port {PORT}");

    clients
        .for_each_concurrent(None, drop_join_handle(dump))
        .await
}
