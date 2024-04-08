use std::{iter::repeat, net::SocketAddr, time::Duration};

/// Implements the core traits that power providers
///
/// These traits allow us to go from `Config -> Provider -> Server`
/// while supporting things like on-demand binary downloading
use crate::{
    error::{Error, Result},
    proxy::Proxy,
    to_uri,
};
use async_trait::async_trait;
use cubist_config::{network::CommonConfig, secret::SecretUrl, CredConfig};
use cubist_proxy::transformer::eth_creds::{build_wallets, EthProxyConfig};
use cubist_util::tasks::retry;
use futures::{
    future::{FutureExt, TryJoinAll},
    select, Future,
};
use reqwest::Client;
use serde_json::{json, Value};
use tokio::process::Child;
use tracing::{debug, trace};

use ethers::prelude::*;
use ethers::providers;

use crate::resource::Downloadable;

pub mod avalanche;
pub mod ethereum;
pub mod polygon;
pub mod stellar;

#[async_trait]
pub trait Provider {
    /// The name of the provider, used for printing info
    /// Note, this really should be an associated constant but Rust doesn't allow that (yet)
    fn name(&self) -> &str;

    /// Whether this is a localnet chain provider.
    fn is_local(&self) -> bool;

    /// How long it typically takes to bootstrap
    fn bootstrap_eta(&self) -> Duration;

    /// The url where the provided service will be found
    fn url(&self) -> SecretUrl;

    /// Do any system checking: do we have the binary we need, is it possible to
    /// get it, etc.
    ///
    /// An error is considered unrecoverable while a success can indicate work to be done
    fn preflight(&self) -> Result<Vec<&Downloadable>>;

    /// Credentials used to configure wallets
    fn credentials(&self) -> Vec<CredConfig>;

    /// Starts a server according to the config. Assumes that preflights have been properly handled
    async fn start(&self) -> Result<Box<dyn Server>>;
}

impl dyn Provider {
    /// Returns all configured wallets for this provider
    pub fn wallets(&self) -> Result<Vec<LocalWallet>> {
        let wallets = build_wallets(self.credentials().iter())?;
        Ok(wallets)
    }
}

#[async_trait]
pub trait Server: Send {
    /// Returns the process id of the server process
    fn pid(&self) -> Option<u32>;

    /// Terminates the server
    async fn kill(&mut self) -> Result<()>;

    /// A future that returns when the server is ready to use
    async fn available(&mut self) -> Result<()>;

    /// Initializes the server (e.g., funds accounts) after the server
    /// is ready to use (i.e., after [`Self::available`]
    /// completes).
    async fn initialize(&mut self) -> Result<()>;
}

/// Provider for a remote endpoint
pub struct RemoteProvider {
    pub name: String,
    pub config: CommonConfig,
}

#[async_trait]
impl Provider for RemoteProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_local(&self) -> bool {
        false
    }

    fn bootstrap_eta(&self) -> Duration {
        Duration::from_millis(400)
    }

    /// Returns either the remote endpoint or a local proxy endpoint if proxy is configured.
    fn url(&self) -> SecretUrl {
        match &self.config.proxy {
            Some(proxy) => {
                let mut url = url::Url::parse("http://127.0.0.1").unwrap();
                url.set_port(Some(proxy.port)).unwrap();
                url.into()
            }
            None => self.config.url.clone(),
        }
    }

    /// Nothing to download for a remote provider.
    fn preflight(&self) -> Result<Vec<&Downloadable>> {
        Ok(vec![])
    }

    /// Return proxy creds, if any
    fn credentials(&self) -> Vec<CredConfig> {
        match &self.config.proxy {
            Some(p) => p.creds.clone(),
            None => vec![],
        }
    }

    /// Starts proxy if proxy is configured, otherwise no-op.
    async fn start(&self) -> Result<Box<dyn Server>> {
        let proxy = match &self.config.proxy {
            Some(p) => {
                let from = SocketAddr::from(([127, 0, 0, 1], p.port));
                let to = &self.config.url;
                let url_exposed = to.expose_url()?;
                let config = EthProxyConfig {
                    creds: self.credentials(),
                    chain_id: p.chain_id,
                    onchain_uri: Some(to_uri(&url_exposed)),
                };
                debug!("Starting proxy {from} -> {to}");
                Some(Proxy::new(from, &url_exposed, config)?)
            }
            None => None,
        };

        Ok(Box::new(RemoteServer { proxy }))
    }
}

/// Remote server that needs no initialization
pub struct RemoteServer {
    proxy: Option<Proxy>,
}

#[async_trait]
impl Server for RemoteServer {
    /// No pid for a remote process
    fn pid(&self) -> Option<u32> {
        None
    }

    /// Drops the proxy.
    async fn kill(&mut self) -> Result<()> {
        self.proxy.take();
        Ok(())
    }

    /// Instantly available.
    async fn available(&mut self) -> Result<()> {
        Ok(())
    }

    /// Nothing to initialize for a remote endpoint.
    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

async fn eth_available(url: SecretUrl, name: String) -> Result<()> {
    // 40s in 200ms increments
    let delays = repeat(Duration::from_millis(200)).take(200);

    let res = retry(delays, move || {
        let client = Client::new();
        let url = url.clone();
        async move {
            let response = client
                .post(url.expose_url()?)
                .header("Content-Type", "application/json")
                .body(
                    json!({
                        "jsonrpc": "2.0",
                        "method":  "eth_gasPrice",
                        "params": [],
                        "id": 73
                    })
                    .to_string(),
                )
                .send()
                .await?
                .error_for_status()?;
            let payload = response.json::<Value>().await?;
            match payload.get("result") {
                Some(gas_price) => Ok(gas_price.to_string()),
                None => Err(eyre::eyre!("No result returned: {payload}")),
            }
        }
    })
    .await;

    match res {
        Ok(gas_price) => trace!("Gas price on {name}: {gas_price}"),
        Err(e) => Err(Error::ServerTimeout(name, format!("{e}")))?,
    };

    Ok(())
}

/// Funds a list of wallets using a developer account
async fn eth_fund_wallets(
    provider: &providers::Provider<Http>,
    dev_account: H160,
    to_fund: &[H160],
) -> Result<()> {
    to_fund
        .iter()
        .map(|addr| async move {
            tracing::debug!("Funding {}", addr);
            let tx = TransactionRequest::new()
                .from(dev_account)
                .to(*addr)
                // TODO: Making funding amount configurable
                .value("21E19E0C9BAB2400000"); // = "1000000000000000000000000"
            provider
                .send_transaction(tx, None)
                // Wait until the transaction has been sent
                .await
                .map_err(Error::EthersProviderError)?
                // Wait until the transaction has been resolved
                .await
                .map_err(Error::EthersProviderError)
        })
        .collect::<TryJoinAll<_>>()
        .await?;
    Ok(())
}

/// Adds a method to run a dependent future in parallel to a given process.
#[async_trait]
pub trait WhileRunning<TR> {
    /// Run a future as long as this process is running. If the process terminates before the future
    /// completes, this method returns a [Error::ProcessTerminated] error.
    async fn while_running(
        &mut self,
        f: impl Future<Output = TR> + Send,
        name: String,
    ) -> Result<TR>;
}

#[async_trait]
impl<TR> WhileRunning<TR> for Child {
    async fn while_running(
        &mut self,
        f: impl Future<Output = TR> + Send,
        name: String,
    ) -> Result<TR> {
        select! {
            r = self.wait().fuse() => Err(Error::ProcessTerminated(name, r.unwrap())),
            r = f.fuse() => Ok(r),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::{pending, ready};
    use tokio::process::Command;

    #[tokio::test]
    async fn process_terminates_before_future() {
        let mut cmd = Command::new("echo").spawn().expect("failed to run process");
        if let Err(Error::ProcessTerminated(name, exit_status)) =
            cmd.while_running(pending::<()>(), "foo".to_string()).await
        {
            assert_eq!(name, "foo");
            assert!(exit_status.success());
        } else {
            panic!("Expected server termination");
        }
    }

    #[tokio::test]
    async fn future_terminates_before_process() {
        let mut cmd = Command::new("sleep")
            .arg("10")
            .spawn()
            .expect("failed to run process");
        assert!(cmd
            .while_running(ready(123), "foo".to_string())
            .await
            .is_ok());
    }
}
