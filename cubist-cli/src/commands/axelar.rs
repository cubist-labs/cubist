use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use color_eyre::owo_colors::OwoColorize;
use cubist_config::{
    paths::hex,
    secret::{Secret, SecretUrl},
    Config,
};
use cubist_localchains::{
    provider::{Provider, WhileRunning},
    resource::DEFAULT_CACHE,
};
use cubist_util::{
    js_pkg_manager::{JsPkgManager, Npm},
    tera::TeraEmbed,
};
use ethers_core::k256::SecretKey;
use eyre::{Context, ContextCompat, Result};
use lazy_static::lazy_static;
use scopeguard::defer;
use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, Serializer};
use tera::Tera;
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
};
use tracing::trace;

use crate::CubeTemplates;

const AXELAR_LOCAL_DEV_PKG: &str = "@axelar-network/axelar-local-dev";

lazy_static! {
    /// JavaScript templates used to generate Axelar relayer
    pub static ref RELAYER_TPL: Tera = CubeTemplates::tera_from_prefix("axelar/");

    /// Node package dependencies needed to run the Axelar relayer
    pub static ref AXELAR_DEPS: HashSet<String> = [ String::from(AXELAR_LOCAL_DEV_PKG) ].into();

    /// Directory in which the dependencies are installed (and where
    /// the relayer will run)
    pub static ref AXELAR_DIR: PathBuf = DEFAULT_CACHE.join("axelar");
}

#[derive(Serialize)]
struct Chain {
    /// Chain name (e.g., target name)
    pub name: String,
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
    /// Node process running Axelar relayer
    child: Child,
}

impl Relayer {
    /// Waits until the relayer process completes.
    pub async fn run_to_completion(mut self) -> Result<()> {
        let exit_status = self.child.wait().await?;
        println!("Exit status: {exit_status}");
        Ok(())
    }
}

/// Spin up relaying for all shim contracts defined in this Cubist
/// project using Axelar relayer.
///
/// # Returns
///
/// A future that completes when the relayer is up and running
/// and all Axelar contracts have been deployed to target chains.
pub async fn start(config: &Config) -> Result<Relayer> {
    println!("{} Axelar dependencies", "Installing".bold().green());
    install_deps()?;

    println!("{} Axelar relayer", "Starting".bold().green());
    let relayer = launch(config).await?;

    println!("{} Axelar relayer", "Ready".bold().green());
    Ok(relayer)
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

async fn launch(config: &Config) -> Result<Relayer> {
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
    let program: SecretString = render(config, &ready_file)?;
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(program.expose_secret().as_bytes()).await?;
    drop(stdin); // must close stdin to proceed
    drop(program); // zeroize secrets in 'program' asap

    // wait until Axelar relayer is ready
    child
        .while_running(wait_ready(&ready_file), "axelar".into())
        .await?;

    Ok(Relayer { child })
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
            name: provider.name().to_string(),
            private_key: hex(&wallet.signer().to_bytes()).into(),
            output_file: paths.for_target(target).axelar_manifest.clone(),
        });
    }
    Ok(chains)
}

fn render(config: &Config, ready_file: &Path) -> Result<SecretString> {
    let chains = configure_chains(config)?;
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
