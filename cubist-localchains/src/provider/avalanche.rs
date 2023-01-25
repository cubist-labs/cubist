use async_trait::async_trait;
use cubist_config::{
    network::{AvalancheConfig, CommonConfig, CredConfig, PrivateKeyConfig, SubnetInfo},
    secret::SecretUrl,
};
use cubist_proxy::transformer::eth_creds::EthProxyConfig;
use cubist_util::{net::next_available_port, tasks::retry};
use hyper::Uri;
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::iter::repeat;
use std::net::SocketAddr;
use std::time::Duration;
use std::{path::PathBuf, process::Stdio};
use tempdir::TempDir;
use tokio::process::{Child, Command};
use tracing::trace;

use crate::error::ProviderError;
use crate::to_uri;
use crate::{
    config::Config,
    error::{Error, Result},
    proxy::Proxy,
    resource::{resource_for_current_machine, Downloadable},
    start_error, UrlExt,
};

use super::{eth_available, Provider, Server, WhileRunning};
use crate::tracing::{child_stdio, trace_stdout};

const DEFAULT_AVALANCHE_PORT: u16 = 8545;

// https://docs.avax.network/quickstart/fund-a-local-test-network
const DEFAULT_AVALANCHE_ACCOUNT: &str = "8db97C7cEcE249c2b98bDC0226Cc4C2A57BF52FC";
const DEFAULT_AVALANCHE_KEY: &str =
    "56289e99c94b6912bfc12adc093c9b51124f0dc54ac7a766b2bc5ccf558d8027";

impl Config for AvalancheConfig {
    fn name(&self) -> &str {
        if self.subnets.is_empty() {
            "avalanche"
        } else {
            "ava_subnet"
        }
    }

    fn common(&self) -> CommonConfig {
        self.common.clone()
    }

    fn local_provider(&self) -> Result<Box<dyn Provider>> {
        assert!(
            self.common.url.is_loopback()?,
            "Cannot start node on remote machine."
        );
        let exe = resource_for_current_machine("avalanchego")?;
        let prov = AvalancheProvider::new(exe, self.clone());
        Ok(Box::new(prov))
    }
}

struct AvalancheProvider {
    exe: Downloadable,
    config: AvalancheConfig,
    avalanchego_path: PathBuf,
    avalanchego_plugins_dir: PathBuf,
    subnet_evm_path: PathBuf,
}

impl AvalancheProvider {
    fn new(exe: Downloadable, config: AvalancheConfig) -> Self {
        let avalanchego_path = Self::find_binary(&exe, "avalanchego");
        let avalanchego_plugins_dir = avalanchego_path.with_file_name("plugins");
        let subnet_evm_path = Self::find_binary(&exe, "subnet-evm");
        AvalancheProvider {
            exe,
            config,
            avalanchego_path,
            avalanchego_plugins_dir,
            subnet_evm_path,
        }
    }

    fn find_binary(exe: &Downloadable, name: &str) -> PathBuf {
        // find the avalanchego binary path
        let os_name = OsStr::new(name);
        let rel_path = &exe
            .binaries
            .iter()
            .find(|(path, _)| path.file_name() == Some(os_name))
            .unwrap_or_else(|| panic!("binary {name} not found"))
            .0;
        exe.destination_dir.join(rel_path)
    }

    pub(crate) fn create_proxy_config(chain_id: u32, onchain_uri: Option<Uri>) -> EthProxyConfig {
        EthProxyConfig {
            onchain_uri,
            chain_id,
            creds: creds(),
        }
    }

    /// Fewer than 4 nodes will result in an unhealthy network.
    fn gen_custom_node_configs(start_port: u16, num_nodes: Option<u16>) -> Result<Value> {
        let num_nodes = num_nodes.unwrap_or(5);
        if num_nodes < 4 {
            let msg = format!("A healthy Avalanche network requires at least 4 nodes, {num_nodes} specified instead");
            return Err(Error::ProviderError(ProviderError::SetupError(msg)));
        }

        let (custom_node_configs, _) =
            (1..=num_nodes).fold((json!({}), start_port), |mut acc, i| {
                let port = acc.1;
                let next_port = next_available_port(acc.1);
                let node_config = json!({
                    "http-port": port,
                    "staking-port": next_port,
                });
                acc.0[format!("node{i}")] = node_config.to_string().into();
                (acc.0, next_available_port(next_port))
            });
        Ok(custom_node_configs)
    }

    /// Converts a given [`SubnetInfo`] to [`BlockchainSpec`],
    /// performing + performs necessary side effects:
    ///
    /// * create a genesis file in the supplied temp dir (referenced from the returned `BlockchainSpec`)    
    /// * move the `subnet-evm` binary to `avalanchego/plugins/{vm_name} (necessary for subnet execution)
    ///
    /// # Arguments
    /// * `temp_dir` - dir into which to save generated genesis file
    /// * `sub` - subnet info
    fn prepare_blockchain_spec(
        &self,
        temp_dir: &TempDir,
        sub: &SubnetInfo,
    ) -> Result<BlockchainSpec> {
        // write a genesis file to a temp folder
        let genesis = temp_dir
            .path()
            .join(format!("genesis-{}.json", sub.vm_name));
        std::fs::write(
            &genesis,
            Self::subnet_evm_genesis(sub.chain_id, DEFAULT_AVALANCHE_ACCOUNT).to_string(),
        )
        .map_err(|e| Error::FsError("Failed to create genesis file", genesis.clone(), e))?;

        // copy 'subnet-evm' to the plugin location corresponding to vm_name
        let plugin_path = self
            .avalanchego_path
            .with_file_name("plugins")
            .join(&sub.vm_id);
        std::fs::copy(&self.subnet_evm_path, &plugin_path)
            .map_err(|e| Error::FsError("Failed to copy evm plugin", plugin_path, e))?;

        Ok(BlockchainSpec {
            vm_name: sub.vm_name.clone(),
            genesis,
        })
    }

    /// Returns a chain genesis JSON value with all defaults except for chain_id and a funded account.
    fn subnet_evm_genesis(chain_id: u32, acc: &str) -> Value {
        // TODO: turn into struct and make general
        json!({
            "config": {
                "chainId": chain_id,
                "feeConfig": {
                    "gasLimit": 8000000,
                    "targetBlockRate": 2,
                    "minBaseFee": 25000000000u64,
                    "targetGas": 15000000,
                    "baseFeeChangeDenominator": 36,
                    "minBlockGasCost": 0,
                    "maxBlockGasCost": 1000000,
                    "blockGasCostStep": 200000
                },
                "homesteadBlock": 0,
                "eip150Block": 0,
                "eip150Hash": "0x2086799aeebeae135c246c65021c82b4e15a2c451340993aacfd2751886514f0",
                "eip155Block": 0,
                "eip158Block": 0,
                "byzantiumBlock": 0,
                "constantinopleBlock": 0,
                "petersburgBlock": 0,
                "istanbulBlock": 0,
                "muirGlacierBlock": 0,
                "subnetEVMTimestamp": 0
            },
            "nonce": "0x0",
            "timestamp": "0x0",
            "extraData": "0x",
            "gasLimit": "0x7a1200",
            "difficulty": "0x0",
            "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "coinbase": "0x0000000000000000000000000000000000000000",
            "alloc": {
                acc: {
                    "balance": "0xd3c21bcecceda1000000"
                }
            },
            "airdropHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "airdropAmount": null,
            "number": "0x0",
            "gasUsed": "0x0",
            "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "baseFeePerGas": null
        })
    }
}

/// Struct corresponding to the `--blockchain-specs` command-line
/// argument of `avalanche-network-runner`, which is a JSON-encoded
/// array of binary tuples: (VM_name, path_to_a_genesis_file).
#[derive(Debug, Serialize)]
struct BlockchainSpec {
    /// VM name corresponding to the subnet to be created
    vm_name: String,
    /// Path to a genesis file for the subnet
    genesis: PathBuf,
}

fn creds() -> Vec<CredConfig> {
    vec![CredConfig::PrivateKey(PrivateKeyConfig {
        hex: DEFAULT_AVALANCHE_KEY.to_string().into(),
    })]
}

#[async_trait]
impl Provider for AvalancheProvider {
    fn name(&self) -> &str {
        self.config.name()
    }

    fn bootstrap_eta(&self) -> Duration {
        if self.config.subnets.is_empty() {
            Duration::from_secs(15)
        } else {
            Duration::from_secs(75)
        }
    }

    fn url(&self) -> SecretUrl {
        self.config.common.url.clone()
    }

    fn preflight(&self) -> Result<Vec<&Downloadable>> {
        // delete stale plugins first
        if let Ok(read_dir) = std::fs::read_dir(&self.avalanchego_plugins_dir) {
            for de in read_dir.filter_map(|e| e.ok()) {
                if de.path().is_file() && de.path().file_name() != Some(OsStr::new("evm")) {
                    let result = std::fs::remove_file(de.path());
                    trace!(
                        "Deleting stale plugin '{}' returned {result:?}",
                        de.path().display()
                    );
                }
            }
        }
        Ok(vec![&self.exe])
    }

    fn credentials(&self) -> Vec<CredConfig> {
        creds()
    }

    async fn start(&self) -> Result<Box<dyn super::Server>> {
        let proxy_port = self
            .config
            .common
            .url
            .port()
            .unwrap_or(DEFAULT_AVALANCHE_PORT);

        let temp_dir = TempDir::new("ava-data")
            .map_err(|e| Error::FsError("Failed to create temp dir", "ava-data".into(), e))?;

        let blockchain_specs = self
            .config
            .subnets
            .iter()
            .map(|sub| self.prepare_blockchain_spec(&temp_dir, sub))
            .collect::<Result<Vec<BlockchainSpec>>>()?;

        // start the 'avalanche-network-runner'
        let server_port = next_available_port(proxy_port);
        let grpc_port = next_available_port(server_port);
        let ava_port = next_available_port(grpc_port);
        let mut child = Command::new(&self.exe.destination())
            .args([
                "server",
                &format!("--port=:{}", server_port),
                &format!("--grpc-gateway-port=:{}", grpc_port),
            ])
            .stdout(child_stdio())
            .stderr(child_stdio())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                start_error!(
                    "Unable to start avalanche server (at '{}') : {}",
                    &self.exe.destination().display(),
                    e
                )
            })?;

        trace_stdout(self.config.name(), &mut child).await;

        let custom_node_configs =
            Self::gen_custom_node_configs(ava_port, Some(self.config.num_nodes))?;

        // send an RPC message to the daemon to start a network with
        // 'num_nodes' nodes (this process exits right after sending a
        // message to the daemon, without waiting for the network to
        // get boostrapped)
        let mut start_cmd = Command::new(&self.exe.destination())
            .args([
                "control",
                "start",
                format!("--endpoint=:{}", server_port).as_str(),
            ])
            .arg("--root-data-dir")
            .arg(temp_dir.path())
            .arg("--avalanchego-path")
            .arg(&self.avalanchego_path)
            .arg("--custom-node-configs")
            .arg(custom_node_configs.to_string())
            .arg("--blockchain-specs")
            .arg(serde_json::to_string(&blockchain_specs).unwrap())
            .stdout(child_stdio())
            .stderr(child_stdio())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                start_error!(
                    "Unable to start avalanche server (at '{}') : {}",
                    &self.exe.destination().display(),
                    e
                )
            })?;
        trace_stdout(&format!("{}/start", self.config.name()), &mut start_cmd).await;

        let exit_status = start_cmd
            .wait()
            .await
            .map_err(|e| start_error!("Unable to start avalanche network: {}", e))?;
        if !exit_status.success() {
            let e = ProviderError::StartError(format!(
                "Failed to start avalanche network. Exit code: {:?}",
                exit_status
            ));
            return Err(Error::ProviderError(e));
        }

        let (endpoint, chain_id) = self.config.eth_endpoint_and_chain_id();
        let ava_url_exposed = self.config.common.url.expose_url_and_update(
            None,
            Some(ava_port),
            Some(endpoint.as_str()),
        )?;
        Ok(Box::new(AvalancheNetworkRunnerServer {
            process: child,
            data_dir: Some(temp_dir),
            proxy: Some(Proxy::new(
                SocketAddr::from(([127, 0, 0, 1], proxy_port)),
                &ava_url_exposed,
                Self::create_proxy_config(chain_id, Some(to_uri(&ava_url_exposed))),
            )?),
            config: AvalancheNetworkRunnerConfig {
                exe: self.exe.destination(),
                rpc_endpoint: ava_url_exposed.into(),
                anr_server_http_port: server_port,
                anr_server_grpc_port: grpc_port,
                config: self.config.clone(),
            },
        }))
    }
}

struct AvalancheNetworkRunnerConfig {
    exe: PathBuf,
    rpc_endpoint: SecretUrl,
    anr_server_http_port: u16,
    anr_server_grpc_port: u16,
    config: AvalancheConfig,
}

struct AvalancheNetworkRunnerServer {
    proxy: Option<Proxy>,
    data_dir: Option<TempDir>,
    process: Child,
    config: AvalancheNetworkRunnerConfig,
}

impl AvalancheNetworkRunnerConfig {
    /// Pings this Avalanche network and returns `Ok(())` if the
    /// network is healhty and [`Error::ServerTimeout`] otherwise.
    ///
    /// The ping returns immediately, i.e., it does not wait for the
    /// network to become healthy.
    ///
    /// The network is healthy if all nodes are healthy and all custom
    /// chains are up and running.
    async fn is_healthy(&self, client: Client) -> Result<()> {
        let mut url = self.rpc_endpoint.expose_url()?;
        url.set_path("v1/control/status");
        url.set_port(Some(self.anr_server_grpc_port))
            .expect("Could not set port");

        async move {
            let response = client
                .post(url)
                .header("Content-Type", "application/json")
                .body("")
                .send()
                .await?;
            if let Err(e) = response.error_for_status_ref() {
                return Err(eyre::eyre!(
                    "Error: {}\n{e}\n{:?}",
                    response.status(),
                    response.text().await
                ));
            };
            let payload = response.json::<Value>().await?;
            let get_bool = |prop_name| {
                payload
                    .get("clusterInfo")
                    .and_then(|v| v.get(prop_name))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            };

            if get_bool("healthy")
                && (self.config.subnets.is_empty() || get_bool("customChainsHealthy"))
            {
                Ok(())
            } else {
                Err(eyre::eyre!("Not healthy yet: {payload}"))
            }
        }
        .await
        .map_err(|e| Error::ServerTimeout(self.config.name().to_string(), format!("{e}")))
    }

    /// Keeps calling [`Self::is_healthy`] until it returns `Ok` or the
    /// timeout of 3 minutes expires.
    async fn until_healthy(&self) -> Result<()> {
        // 3 min worth of waiting
        let delays = repeat(Duration::from_millis(1000)).take(5 * 480);
        let client = Client::new();
        retry(delays, || self.is_healthy(client.clone())).await
    }
}

impl AvalancheNetworkRunnerServer {
    /// Whether the ANR process is still running.
    fn is_running(&mut self) -> bool {
        matches!(self.process.try_wait(), Ok(None))
    }

    /// Stops the currently running Avalanche network by sending a
    /// 'control stop' RPC message to the avalanche network runner daemon.
    fn try_stop_network(&mut self) {
        // if still running, send 'control stop'
        if !self.is_running() {
            return;
        }

        trace!("Stopping avalanche network");
        let ret = std::process::Command::new(&self.config.exe)
            .args([
                "control",
                "stop",
                "--dial-timeout=100ms",
                format!("--endpoint=:{}", self.config.anr_server_http_port).as_str(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|mut ch| ch.wait());
        trace!("Calling 'stop' returned: {ret:?}");
    }
}

#[async_trait]
impl Server for AvalancheNetworkRunnerServer {
    fn pid(&self) -> Option<u32> {
        self.process.id()
    }

    async fn kill(&mut self) -> Result<()> {
        // first stop the network gracefully (by calling 'avalanche-network-runner control stop')
        self.try_stop_network();

        // next kill the server process
        let pid = self.process.id().unwrap_or_default();
        let ret = self.process.kill().await;
        trace!("Calling 'kill({pid})' returned {ret:?}");

        self.proxy.take();
        self.data_dir.take();
        Ok(())
    }

    async fn available(&mut self) -> Result<()> {
        let name = "avalanche".to_string();

        // wait until healthy
        self.process
            .while_running(self.config.until_healthy(), name.clone())
            .await??;

        // check the eth endpoint as well, just in case
        self.process
            .while_running(
                eth_available(self.config.rpc_endpoint.clone(), name.clone()),
                name,
            )
            .await??;
        Ok(())
    }

    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Drop for AvalancheNetworkRunnerServer {
    fn drop(&mut self) {
        self.try_stop_network();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubist_proxy::transformer::eth_creds::build_wallets;
    use ethers::{
        signers::{LocalWallet, Signer},
        types::H160,
    };

    #[test]
    pub fn test_proxy_creds() {
        let chain_id = 123;
        let cfg = AvalancheProvider::create_proxy_config(chain_id, None);
        assert_eq!(1, cfg.creds.len());
        let wallets = build_wallets(cfg.creds.iter()).unwrap();
        assert_eq!(1, wallets.len());
        let wallet: &LocalWallet = &wallets[0];
        assert_eq!(
            DEFAULT_AVALANCHE_ACCOUNT.parse::<H160>().unwrap(),
            wallet.address()
        );
    }

    #[test]
    pub fn test_gen_custom_node_configs() {
        let start_port = 1233;
        let num_nodes = 4;
        let cfg = AvalancheProvider::gen_custom_node_configs(start_port, Some(num_nodes)).unwrap();
        let get_prop_value = |node_name: &str, prop_name: &str| {
            let node_str = cfg
                .get(node_name)
                .unwrap_or_else(|| panic!("'{node_name}' not found"))
                .as_str()
                .unwrap_or_else(|| panic!("'{node_name}' is not string"));
            let node_val: Value = serde_json::from_str(node_str).unwrap_or_else(|e| {
                panic!("'{node_name}' is not a valid json; node value: {node_str}; err: {e}")
            });
            node_val
                .get(prop_name)
                .unwrap_or_else(|| {
                    panic!("Property '{prop_name}' not found in '{node_str}' for '{node_name}'")
                })
                .clone()
        };

        // 'node1' must have the exact same start port
        assert_eq!(
            Some(start_port as u64),
            get_prop_value("node1", "http-port").as_u64()
        );
        let mut last_port = start_port as u64 - 1;
        for i in 1..=num_nodes {
            let node_name = format!("node{i}");
            let port_val = get_prop_value(&node_name, "http-port");
            let port = port_val
                .as_u64()
                .unwrap_or_else(|| panic!("http-port is not a number: {port_val:?}"));
            assert!(port > last_port);
            let staking_port_val = get_prop_value(&node_name, "staking-port");
            let staking_port = staking_port_val
                .as_u64()
                .unwrap_or_else(|| panic!("staking-port is not a number: {staking_port_val:?}"));
            assert!(
                staking_port > port,
                "staking-port: {staking_port}; http-port: {port}"
            );
            last_port = staking_port;
        }
    }
}
