use std::{iter::repeat, time::Duration};

use async_trait::async_trait;
use cubist_config::{
    network::{CommonConfig, CredConfig, IdentityConfig, StellarConfig},
    secret::SecretUrl,
};
use cubist_util::tasks::retry;
use reqwest::Client;
use serde_json::Value;
use tokio::process::{Child, Command};
use tracing::trace;

use crate::{
    config::Config,
    error::ProviderError,
    error::{Error, Result},
    resource::Downloadable,
    start_error, UrlExt,
};

use super::{Provider, Server};
use crate::tracing::child_stdio;

const DEFAULT_STELLAR_PORT: u16 = 10545;

impl Config for StellarConfig {
    fn name(&self) -> &str {
        "stellar"
    }

    fn common(&self) -> CommonConfig {
        self.common.clone()
    }

    fn local_provider(&self) -> Result<Box<dyn Provider>> {
        assert!(
            self.common.url.is_loopback()?,
            "Cannot start node on remote machine."
        );
        let prov = StellarServerProvider {
            config: StellarServerConfig {
                url: self.common.url.clone(),
                identities: self.identities.clone(),
            },
        };
        Ok(Box::new(prov))
    }
}

struct StellarServerProvider {
    config: StellarServerConfig,
}

struct StellarServerConfig {
    url: SecretUrl,
    identities: Vec<String>,
}

#[async_trait]
impl Provider for StellarServerProvider {
    fn name(&self) -> &str {
        "stellar"
    }

    fn is_local(&self) -> bool {
        true
    }

    fn bootstrap_eta(&self) -> Duration {
        Duration::from_secs(10)
    }

    fn url(&self) -> SecretUrl {
        self.config.url.clone()
    }

    fn preflight(&self) -> Result<Vec<&Downloadable>> {
        Ok(vec![])
    }

    fn credentials(&self) -> Vec<CredConfig> {
        self.config
            .identities
            .iter()
            .map(|identity| {
                CredConfig::Identity(IdentityConfig {
                    identity: identity.clone(),
                })
            })
            .collect()
    }

    async fn start(&self) -> Result<Box<dyn super::Server>> {
        let stellar_port = self.config.url.port().unwrap_or(DEFAULT_STELLAR_PORT);

        let output = Command::new("docker")
            .arg("ps")
            .output()
            .await
            .map_err(|e| start_error!("Unable to start Docker: {}", e))?;
        if !output.status.success() {
            Err(ProviderError::StartError(
                "Unable to connect to Docker".to_string(),
            ))?;
        }

        let child = Command::new("docker")
            .args(["run", "--rm", "-it", "-p"])
            .arg(format!("{}:8000", stellar_port))
            .args([
                "--name",
                "stellar",
                "stellar/quickstart:testing@sha256:0db21654113699288f2ed59d7645734bacb349d766e83815acbb564bb99a4991",
                "--standalone",
                "--enable-soroban-rpc",
            ])
            .stdout(child_stdio())
            .stderr(child_stdio())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| start_error!("Unable to start stellar server: {}", e))?;

        let server_url = self
            .config
            .url
            .expose_url_and_update(None, Some(stellar_port), None)?;

        Ok(Box::new(StellarServer {
            process: child,
            server_url: server_url.into(),
        }))
    }
}

struct StellarServer {
    process: Child,
    server_url: SecretUrl,
}

#[async_trait]
impl Server for StellarServer {
    fn pid(&self) -> Option<u32> {
        self.process.id()
    }

    async fn kill(&mut self) -> Result<()> {
        let result = self.process.kill().await;
        trace!("Killing stellar process returned {result:?}");
        Ok(())
    }

    async fn available(&mut self) -> Result<()> {
        // 40s in 200ms increments
        let delays = repeat(Duration::from_millis(200)).take(200);

        let res = retry(delays, move || {
            let client = Client::new();
            let url = self.server_url.clone();
            async move {
                let response = client
                    .get(url.expose_url()?.join("fee_stats").unwrap())
                    .send()
                    .await?
                    .error_for_status()?;
                let payload = response.json::<Value>().await?;
                match payload.get("last_ledger") {
                    Some(last_ledger) => Ok(last_ledger.to_string()),
                    None => Err(eyre::eyre!("No result returned: {payload}")),
                }
            }
        })
        .await;

        match res {
            Ok(last_ledger) => trace!("Last ledger (stellar): {last_ledger}"),
            Err(e) => Err(Error::ServerTimeout("stellar".to_string(), format!("{e}")))?,
        };

        Ok(())
    }

    async fn initialize(&mut self) -> Result<()> {
        let output = Command::new("soroban")
            .args([
                "config",
                "network",
                "add",
                "standalone",
                "--global",
                "--network-passphrase",
                "Standalone Network ; February 2017",
                "--rpc-url",
            ])
            .arg(format!("{}soroban/rpc", self.server_url))
            .output()
            .await
            .unwrap();
        if !output.status.success() {
            Err(ProviderError::StartError(
                "Unable to configure Stellar network".to_string(),
            ))?;
        }

        Ok(())
    }
}

impl Drop for StellarServer {
    fn drop(&mut self) {
        std::mem::drop(self.kill());
    }
}
