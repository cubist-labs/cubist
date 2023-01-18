use std::{net::SocketAddr, sync::Arc};

use cubist_proxy::{
    listen_on,
    pipeline::eth,
    transformer::eth_creds::{CredProxy, EthProxyConfig},
};
use ethers_providers::StreamExt;
use futures::{
    channel::oneshot::{self, Sender},
    select, FutureExt,
};
use url::Url;

use crate::{error::Error, to_uri};

pub(crate) struct Proxy(Option<Sender<()>>);

impl Proxy {
    /// Starts a proxy at address `from` forwarding requests to url `to`.
    ///
    /// # Arguments
    ///
    /// * `from` - local address on which the proxy should listen
    /// * `to` - ethereum endpoint to which the proxy should forward requests
    /// * `config` - proxy configuration
    ///
    /// # Returns
    ///
    /// Starts a proxy server and returns immediately after. The server
    /// will be stopped once the proxy instance is dropped.
    pub fn new(from: SocketAddr, to: &Url, config: EthProxyConfig) -> Result<Proxy, Error> {
        let uri = to_uri(to);
        let (snd, rcv) = oneshot::channel();
        let proxy = Arc::new(CredProxy::from_cfg(&config)?);
        tokio::spawn(async move {
            let srv = async move {
                let mut listener = listen_on(from);
                while let Some(offchain) = listener.next().await {
                    let uri = uri.clone();
                    let proxy = proxy.clone();
                    tokio::spawn(async move { eth(offchain, uri, proxy).await });
                }
            };

            select! {
                _ = srv.fuse() => {}
                _ = rcv.fuse() => {}
            }
        });

        Ok(Self(Some(snd)))
    }

    fn stop(&mut self) {
        let _taken = self.0.take();
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop()
    }
}
