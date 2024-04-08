use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use color_eyre::owo_colors::OwoColorize;
use cubist_config::{
    axelar_manifest::{AxelarManifest, ChainName},
    paths::hex,
    secret::{Secret, SecretUrl},
    Config,
};
use cubist_localchains::{
    provider::{Provider, WhileRunning},
    resource::DEFAULT_CACHE,
};
use cubist_sdk::gen::backend::{AxelarBackend, AxelarNetwork};
use cubist_util::{
    js_pkg_manager::{JsPkgManager, Npm},
    tera::TeraEmbed,
};
use ethers_core::{k256::SecretKey, types::U256};
use ethers_providers::Middleware;
use eyre::{bail, Context, ContextCompat, Result};
use lazy_static::lazy_static;
use scopeguard::defer;
use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, Serializer};
use tera::Tera;
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
};
use tracing::{debug, trace};

use crate::CubeTemplates;

const AXELAR_LOCAL_DEV_PKG: &str = "@axelar-network/axelar-local-dev@1.2.5";

lazy_static! {
    /// JavaScript templates used to generate Axelar relayer
    pub static ref RELAYER_TPL: Tera = CubeTemplates::tera_from_prefix("axelar/");

    /// Node package dependencies needed to run the Axelar relayer
    pub static ref AXELAR_DEPS: HashSet<String> = [ String::from(AXELAR_LOCAL_DEV_PKG) ].into();

    /// Directory in which the dependencies are installed (and where
    /// the relayer will run)
    pub static ref AXELAR_DIR: PathBuf = DEFAULT_CACHE.join("axelar");

    /// Directory where to look for Axelar's 'testnet.json' and 'mainnet.json' files
    static ref AXELAR_CHAIN_INFO_DIR: PathBuf = AXELAR_DIR.join("node_modules")
        .join("@axelar-network").join("axelar-cgp-solidity").join("info");

    /// Axelar's 'testnet.json' file
    pub static ref AXELAR_TESTNET_INFO: PathBuf = AXELAR_CHAIN_INFO_DIR.join("testnet.json");

    /// Axelar's 'mainnet.json' file
    pub static ref AXELAR_MAINNET_INFO: PathBuf = AXELAR_CHAIN_INFO_DIR.join("mainnet.json");
}

#[derive(Serialize)]
struct Chain {
    /// Chain name (e.g., target name)
    pub name: ChainName,
    /// Whether this is a localnet chain
    pub is_local: bool,
    /// Where to save produced metadata
    pub output_file: PathBuf,
    /// Chain RPC endpoint to connect to
    #[serde(serialize_with = "expose_url")]
    pub url: SecretUrl,
    /// Private key to use for auth when connecting to `url`
    #[serde(serialize_with = "expose_secret")]
    pub private_key: Secret<SecretKey>,
}

/// Must expose secret when serializing because the actual value is forwarded to Axelar.
fn expose_secret<S>(key: &Secret<SecretKey>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let loaded = key.load().unwrap();
    serializer.serialize_str(loaded.expose_secret())
}

/// Must expose secret when serializing because the actual value is forwarded to Axelar.
fn expose_url<S>(key: &SecretUrl, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    key.expose_url().unwrap().serialize(serializer)
}

/// Wrapper around a 'node' process running an Axelar relayer.
pub struct Relayer {
    /// Node process running Axelar relayer or `None` if using remote Axelar relayer.
    child: Option<Child>,
}

impl Relayer {
    /// Waits until the relayer process completes.
    pub async fn run_to_completion(mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let exit_status = child.wait().await?;
            println!("Exit status: {exit_status}");
        }
        Ok(())
    }
}

/// Spin up relaying for all shim contracts defined in this Cubist
/// project using Axelar relayer.
///
/// If all configured networks are local, runs an Axelar relayer
/// locally; otherwise, it doesn't run anything locally but it does
/// check that all networks are either on testnet or mainnet.
///
/// # Returns
///
/// A future that completes when the relayer is up and running
/// and all Axelar contracts have been deployed to target chains.
pub async fn start(config: &Config) -> Result<Relayer> {
    println!("{} Axelar dependencies", "Installing".bold().green());
    install_deps()?;

    let chains = configure_chains(config)?;
    if chains.iter().all(|c| c.is_local) {
        local_relayer(chains).await
    } else if chains.iter().all(|c| !c.is_local) {
        remote_relayer(config, chains).await
    } else {
        bail!("To use the Axelar relayer all chains must be either local or remote")
    }
}

async fn local_relayer(chains: Vec<Chain>) -> Result<Relayer> {
    println!("{} Axelar relayer", "Starting".bold().green());
    let relayer = launch(&chains).await?;

    println!("{} Axelar relayer", "Ready".bold().green());
    Ok(relayer)
}

fn deserialize_axelar_manifest(path: &Path) -> Result<Vec<AxelarManifest>> {
    let json = std::fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read Axelar's chain info file: {}",
            path.display()
        )
    })?;
    let info: Vec<AxelarManifest> = serde_json::from_str(&json).with_context(|| {
        format!(
            "Failed to deserialize `AxelarManifest[]` from {}",
            path.display()
        )
    })?;
    Ok(info)
}

/// Only sets up Axelar manifest files (used subsequently by the SDK).
/// Actual relayer is run externally by Axelar.
async fn remote_relayer(_config: &Config, chains: Vec<Chain>) -> Result<Relayer> {
    fn find(nets: &[AxelarManifest], chain_id: U256) -> Option<&AxelarManifest> {
        nets.iter().find(|n| U256::from(n.chain_id) == chain_id)
    }

    let testnets = deserialize_axelar_manifest(AXELAR_TESTNET_INFO.as_path())?;
    let mainnets = deserialize_axelar_manifest(AXELAR_MAINNET_INFO.as_path())?;

    let mut matching_testnets = vec![];
    let mut matching_mainnets = vec![];

    // query each chain to find its chainId, then find its manifest in either testnet.json or mainnet.json
    for chain in &chains {
        let chain_id = ethers_providers::Provider::try_from(chain.url.expose_url()?.as_str())
            .with_context(|| format!("Cannot connect to {}", chain.name))?
            .get_chainid()
            .await
            .with_context(|| format!("Cannot get chain ID for {}", chain.name))?;
        if let Some(n) = find(&testnets, chain_id) {
            matching_testnets.push(n);
        } else if let Some(n) = find(&mainnets, chain_id) {
            matching_mainnets.push(n);
        } else {
            bail!(
                "No Axelar network found for {} (chain id: {chain_id})",
                chain.name
            );
        }
    }

    // all chains must be either in testnet or mainnet
    let (matching_nets, kind) = if matching_testnets.len() == chains.len() {
        (matching_testnets, "testnet")
    } else if matching_mainnets.len() == chains.len() {
        (matching_mainnets, "mainnet")
    } else {
        bail!("All chains must be either on testnet or mainnet but the config file contains {} on mainnet and {} on testnet",
              matching_mainnets.len(), matching_testnets.len());
    };

    // save discovered Axelar manifest files to Cubist's deploy dir
    for (chain, manifest) in chains.iter().zip(matching_nets.iter()) {
        let manifest_json = serde_json::to_string_pretty(manifest)?;
        let parent_dir = chain.output_file.parent().unwrap();
        std::fs::create_dir_all(parent_dir)
            .with_context(|| format!("Failed to create dir: {}", parent_dir.display()))?;
        std::fs::write(&chain.output_file, manifest_json).with_context(|| {
            format!(
                "Failed to save manifest for chain {} to {}",
                chain.name,
                chain.output_file.display()
            )
        })?;
        debug!(
            "Saved Axelar manifest for {} to {}",
            chain.name,
            chain.output_file.display()
        );
    }

    println!("{} Axelar {} relayer", "Using".bold().green(), kind.blue());
    Ok(Relayer { child: None })
}

/// Install dependencies needed to run Axelar relayer
pub fn install_deps() -> Result<()> {
    let dir: &Path = &AXELAR_DIR;
    std::fs::create_dir_all(dir).with_context(|| format!("Create dir: {}", dir.display()))?;
    // use Npm and not Yarn because Yarn may complain if node version
    // is too high for hardhat (which is a dependency of axelar)
    Npm.install(dir, &AXELAR_DEPS)?;
    Ok(())
}

async fn launch(chains: &Vec<Chain>) -> Result<Relayer> {
    // launch plain 'node' process in AXELAR_DIR (where node_modules
    // have already been installed)
    let mut child = Command::new("node")
        .current_dir(&*AXELAR_DIR)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to start 'node'. Ensure 'node' is installed")?;

    let ready_file = AXELAR_DIR.join(format!("axelar.{}.ready", child.id().unwrap()));
    defer! { std::fs::remove_file(&ready_file).ok(); }

    // flush the Axelar program to node's stdin instead of saving it
    // to a file (because the program contains secrets)
    let program: SecretString = render(chains, &ready_file)?;
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(program.expose_secret().as_bytes()).await?;
    drop(stdin); // must close stdin to proceed
    drop(program); // zeroize secrets in 'program' asap

    // wait until Axelar relayer is ready
    child
        .while_running(wait_ready(&ready_file), "axelar".into())
        .await?;

    Ok(Relayer { child: Some(child) })
}

/// Return a vector of [Chain]; each instance in that vector
/// contains all the info needed to configure Axelar.
fn configure_chains(config: &Config) -> Result<Vec<Chain>> {
    let mut chains = vec![];
    let paths = config.paths();
    for target in config.targets() {
        let endpoint = config
            .network_for_target(target)
            .with_context(|| format!("Network must be defined for target {target}"))
            .unwrap();
        let provider: Box<dyn Provider> = endpoint.try_into()?;
        let wallets = provider.wallets()?;
        let wallet = wallets
            .first()
            .with_context(|| format!("Must define credentials for target {target}"))
            .unwrap();
        chains.push(Chain {
            url: provider.url(),
            name: AxelarBackend::to_chain_name(target, AxelarNetwork::Localnet),
            is_local: provider.is_local(),
            private_key: hex(&wallet.signer().to_bytes()).into(),
            output_file: paths.for_target(target).axelar_manifest.clone(),
        });
    }
    Ok(chains)
}

fn render(chains: &Vec<Chain>, ready_file: &Path) -> Result<SecretString> {
    let mut tera_ctx = tera::Context::new();
    tera_ctx.insert("chains", &chains);
    tera_ctx.insert("ready_file", ready_file);
    Ok(SecretString::new(
        RELAYER_TPL.render("relay.js.tpl", &tera_ctx)?,
    ))
}

async fn wait_ready(path: &Path) {
    trace!("Waiting for Axelar relayer (watching: {})", path.display());
    while !path.is_file() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    trace!("Axelar relayer ready");
}
