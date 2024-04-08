use std::{net::SocketAddr, time::Duration};

use async_trait::async_trait;
use cubist_config::{
    network::{CommonConfig, CredConfig, EthereumConfig, MnemonicConfig},
    secret::SecretUrl,
};
use cubist_proxy::transformer::eth_creds::EthProxyConfig;
use cubist_util::net::next_available_port;
use secrecy::ExposeSecret;
use tokio::process::{Child, Command};
use tracing::trace;

use crate::{
    config::Config,
    error::Result,
    proxy::Proxy,
    resource::{resource_for_current_machine, Downloadable},
    start_error, to_uri, UrlExt,
};

use super::{eth_available, Provider, Server, WhileRunning};
use crate::tracing::{child_stdio, trace_stdout};

const DEFAULT_ETHEREUM_PORT: u16 = 8545;
const DEFAULT_ETHEREUM_CHAIN_ID: u32 = 31337;

impl Config for EthereumConfig {
    fn name(&self) -> &str {
        "ethereum"
    }

    fn common(&self) -> CommonConfig {
        self.common.clone()
    }

    fn local_provider(&self) -> Result<Box<dyn Provider>> {
        assert!(
            self.common.url.is_loopback()?,
            "Cannot start node on remote machine."
        );
        let prov = AnvilProvider {
            exe: resource_for_current_machine("anvil")?,
            config: AnvilConfig {
                url: self.common.url.clone(),
                mnemonic: self.bootstrap_mnemonic.clone(),
            },
        };
        Ok(Box::new(prov))
    }
}

struct AnvilProvider {
    exe: Downloadable,
    config: AnvilConfig,
}

struct AnvilConfig {
    url: SecretUrl,
    mnemonic: MnemonicConfig,
}

#[async_trait]
impl Provider for AnvilProvider {
    fn name(&self) -> &str {
        "ethereum"
    }

    fn is_local(&self) -> bool {
        true
    }

    fn bootstrap_eta(&self) -> Duration {
        Duration::from_secs(2)
    }

    fn url(&self) -> SecretUrl {
        self.config.url.clone()
    }

    fn preflight(&self) -> Result<Vec<&Downloadable>> {
        Ok(vec![&self.exe])
    }

    fn credentials(&self) -> Vec<CredConfig> {
        vec![CredConfig::Mnemonic(self.config.mnemonic.clone())]
    }

    async fn start(&self) -> Result<Box<dyn super::Server>> {
        let proxy_port = self.config.url.port().unwrap_or(DEFAULT_ETHEREUM_PORT);
        let anvil_port = next_available_port(proxy_port);

        let mut child = Command::new(self.exe.destination())
            .arg("--chain-id")
            .arg(DEFAULT_ETHEREUM_CHAIN_ID.to_string())
            .arg("--port")
            .arg(&anvil_port.to_string())
            .arg("--accounts")
            .arg(self.config.mnemonic.account_count.to_string())
            .arg("--mnemonic")
            .arg(self.config.mnemonic.seed.load()?.expose_secret())
            .arg("--derivation-path")
            .arg(&self.config.mnemonic.derivation_path)
            .stdout(child_stdio())
            .stderr(child_stdio())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                start_error!(
                    "Unable to start anvil server (at {}): {}",
                    &self.exe.destination().display(),
                    e
                )
            })?;

        trace_stdout("anvil", &mut child).await;

        let anvil_url_exposed =
            self.config
                .url
                .expose_url_and_update(None, Some(anvil_port), None)?;
        let proxy = Proxy::new(
            SocketAddr::from(([127, 0, 0, 1], proxy_port)),
            &anvil_url_exposed,
            EthProxyConfig {
                // Safety: URLs are valid URIs
                onchain_uri: Some(to_uri(&anvil_url_exposed)),
                chain_id: DEFAULT_ETHEREUM_CHAIN_ID,
                creds: self.credentials(),
            },
        )?;

        Ok(Box::new(AnvilServer {
            anvil_url: self.url(),
            _proxy: proxy,
            process: child,
        }))
    }
}

struct AnvilServer {
    process: Child,
    _proxy: Proxy,
    anvil_url: SecretUrl,
}

#[async_trait]
impl Server for AnvilServer {
    fn pid(&self) -> Option<u32> {
        self.process.id()
    }

    async fn kill(&mut self) -> Result<()> {
        let result = self.process.kill().await;
        trace!("Killing anvil process returned {result:?}");
        Ok(())
    }

    async fn available(&mut self) -> Result<()> {
        let name = "anvil".to_string();
        self.process
            .while_running(eth_available(self.anvil_url.clone(), name.clone()), name)
            .await??;
        Ok(())
    }

    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Drop for AnvilServer {
    fn drop(&mut self) {
        std::mem::drop(self.kill());
    }
}
