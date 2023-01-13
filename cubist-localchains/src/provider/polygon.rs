use async_trait::async_trait;
use cubist_config::{
    network::{CommonConfig, CredConfig, PolygonConfig},
    secret::SecretUrl,
};
use cubist_proxy::transformer::eth_creds::{build_wallets, EthProxyConfig};
use cubist_util::net::next_available_port;
use secrecy::ExposeSecret;
use std::net::SocketAddr;
use std::time::Duration;
use tempdir::TempDir;
use tracing::trace;

use crate::{
    config::Config,
    error::{Error, ProviderError, Result},
    proxy::Proxy,
    resource::{resource_for_current_machine, Downloadable},
};
use crate::{start_error, to_uri, UrlExt};

use tokio::process::{Child, Command};

use super::{eth_available, eth_fund_wallets, Provider, Server, WhileRunning};
use crate::tracing::{child_stdio, trace_stdout};

use ethers::prelude::*;
use ethers::providers;

const PROVIDER_NAME: &str = "polygon";
const BOR_DEFAULT_PORT: u16 = 9545;
const BOR_CHAIN_ID: u32 = 1337;

impl Config for PolygonConfig {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    fn common(&self) -> CommonConfig {
        self.common.clone()
    }

    fn local_provider(&self) -> Result<Box<dyn Provider>> {
        assert!(
            self.common.url.is_loopback()?,
            "Cannot start node on remote machine."
        );
        let url = self.common.url.clone();
        let config = BorConfig {
            url,
            local_accounts: self.local_accounts.clone(),
        };
        let provider = BorProvider {
            exe: resource_for_current_machine("bor")?,
            config,
        };
        Ok(Box::new(provider))
    }
}

struct BorConfig {
    url: SecretUrl,
    local_accounts: Vec<CredConfig>,
}

pub struct BorProvider {
    exe: Downloadable,
    config: BorConfig,
}

struct BorServer {
    process: Child,
    bor_url: SecretUrl,
    data_dir: Option<TempDir>,
    proxy: Option<Proxy>,
    /// Accounts to fund when initializing the server.
    to_fund: Vec<H160>,
}

impl BorServer {
    /// Creates a provider for communicating with the server
    fn create_provider(&self) -> Result<providers::Provider<Http>> {
        let url = self.bor_url.load()?;
        let provider = providers::Provider::<Http>::try_from(url.expose_secret())
            .map_err(ProviderError::UrlParseError)?
            .interval(Duration::from_millis(10));
        Ok(provider)
    }

    /// Retrieves the developer account, i.e., an account that is pre-funded when the server is
    /// started
    async fn get_dev_account(&self, provider: &providers::Provider<Http>) -> Result<H160> {
        // The developer account is the only account that exists after starting the server
        let dev_account = provider.get_accounts().await?[0];
        let balance = provider.get_balance(dev_account, None).await.unwrap();
        tracing::debug!(
            "Dev account is {} with a balance of {}",
            dev_account,
            balance
        );
        Ok(dev_account)
    }
}

#[async_trait]
impl Server for BorServer {
    fn pid(&self) -> Option<u32> {
        self.process.id()
    }

    async fn kill(&mut self) -> Result<()> {
        let result = self.process.kill().await;
        trace!(
            "Killing bor server (pid = {:?}) returned {result:?}",
            self.process.id()
        );
        self.proxy.take();
        self.data_dir.take();
        Ok(())
    }

    async fn available(&mut self) -> Result<()> {
        let name = PROVIDER_NAME.to_string();
        self.process
            .while_running(eth_available(self.bor_url.clone(), name.clone()), name)
            .await??;
        Ok(())
    }

    async fn initialize(&mut self) -> Result<()> {
        let provider = self.create_provider()?;
        let dev_account = self.get_dev_account(&provider).await?;
        eth_fund_wallets(&provider, dev_account, &self.to_fund).await?;
        Ok(())
    }
}

impl Drop for BorServer {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

#[async_trait]
impl Provider for BorProvider {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    fn bootstrap_eta(&self) -> Duration {
        Duration::from_secs(3)
    }

    fn url(&self) -> SecretUrl {
        self.config.url.clone()
    }

    fn preflight(&self) -> Result<Vec<&Downloadable>> {
        Ok(vec![&self.exe])
    }

    async fn start(&self) -> Result<Box<dyn Server>> {
        // setup before running
        let temp_dir = TempDir::new("bor-data")
            .map_err(|e| Error::FsError("Failed to create temp dir", "bor-data".into(), e))?;
        let data_dir = temp_dir.path();

        let proxy_port = self.config.url.port().unwrap_or(BOR_DEFAULT_PORT);
        let bor_port = next_available_port(proxy_port);

        let creds = &self.config.local_accounts;
        let to_fund: Vec<H160> = creds
            .iter()
            .map(build_wallets)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .map(|w| w.address())
            .collect();

        let mut child = Command::new(&self.exe.destination())
            .arg("--dev")
            .arg("--datadir")
            .arg(data_dir)
            .args([
                "--port",
                "30303",
                "--http",
                "--http.vhosts",
                "*",
                "--http.corsdomain",
                "*",
                "--http.port",
                &bor_port.to_string(),
            ])
            .args(["--http.api", "eth,net,web3,txpool"])
            .arg("--networkid")
            .arg(BOR_CHAIN_ID.to_string())
            .args(["--miner.gasprice", "0"])
            .args([
                "--metrics",
                "--pprof",
                "--pprof.port",
                "7071",
                "--nodiscover",
                "--maxpeers",
                "0",
            ])
            .stdout(child_stdio())
            .stderr(child_stdio())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                start_error!(
                    "Unable to start bor server (at {}): {}",
                    &self.exe.destination().display(),
                    e
                )
            })?;

        trace_stdout("bor", &mut child).await;

        let bor_url_exposed = self
            .config
            .url
            .expose_url_and_update(None, Some(bor_port), None)?;
        Ok(Box::new(BorServer {
            process: child,
            data_dir: Some(temp_dir),
            proxy: Some(Proxy::new(
                SocketAddr::from(([127, 0, 0, 1], proxy_port)),
                &bor_url_exposed,
                EthProxyConfig {
                    onchain_uri: Some(to_uri(&bor_url_exposed)),
                    chain_id: BOR_CHAIN_ID,
                    creds: creds.to_vec(),
                },
            )?),
            bor_url: bor_url_exposed.into(),
            to_fund,
        }))
    }
}

#[cfg(test)]
mod tests {
    use cubist_config::network::{MnemonicConfig, DEFAULT_ETH_DERIVATION_PATH_PREFIX};

    use super::*;

    #[test]
    pub fn test_proxy_creds() {
        let cred = CredConfig::Mnemonic(MnemonicConfig {
            seed: "test test test test test test test test test test test junk"
                .to_string()
                .into(),
            account_count: 5,
            derivation_path: DEFAULT_ETH_DERIVATION_PATH_PREFIX.to_string(),
        });
        let wallets = build_wallets(&cred).unwrap();
        assert_eq!(5, wallets.len());
    }
}
