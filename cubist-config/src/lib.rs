#![warn(missing_docs)]

//! This crate exposes the Cubist app/project configuration interface [`Config`].
//!
//! All cubist applications have a JSON config file `cubist-config.json`, which specifies:
//! - [`type`](Config::type_): the kind of project ([`ProjType`]) the off-chain application code is written in,
//! - [`build_dir`](Config::build_dir): where Cubist will write build output (can be overridden via `CUBIST_BUILD_DIR` env var),
//! - [`deploy_dir`](Config::deploy_dir): where Cubist will generate deployment scripts and information (can be overridden via `CUBIST_DEPLOY_DIR` env var),
//! - [`contracts`](Config::contracts): assignment of contracts to chains (currently per contract source file, not per contract)
//! - [`network_profiles`](Config::network_profiles): named profiles containing network/chain configuration (see [network configuration](network)),
//! - [`current_network_profile`](Config::current_network_profile): currently selected network profile (can be overridden via `CUBIST_NETWORK_PROFILE` env var).
//!
//! Example JSON file:
//! ```
//! # use cubist_config::Config;
//! # use serde_json::{from_str, json};
//! # let cfg_json = json!(
//! {
//!    "type": "TypeScript",
//!    "build_dir": "./build",
//!    "deploy_dir": "./deploy",
//!    "contracts": {
//!       "root_dir": "./contracts",
//!       "targets": {
//!         "avalanche": { "files": [ "./contracts/ava.sol" ] },
//!         "polygon":   { "files": [ "./contracts/poly.sol" ] },
//!         "ethereum":  { "files": [ "./contracts/eth1.sol", "./contracts/eth2.sol" ] }
//!       }
//!    },
//!    "network_profiles": {
//!      "default": {
//!        "avalanche": { "url": "http://localhost:9560", "autostart": true },
//!        "ethereum":  { "url": "http://localhost:7545", "autostart": true },
//!        "polygon":   { "url": "http://localhost:9545", "autostart": true }
//!      },
//!      "dev": {
//!        "avalanche": { "url": "http://otherhost:9560" },
//!        "ethereum":  { "url": "http://otherhost:7545" },
//!        "polygon":   {
//!          "url": "wss://polygon-mumbai.g.alchemy.com/v2/${{env.ALCHEMY_MUMBAI_API_KEY}}",
//!          "proxy": {
//!            "port": 9545,
//!            "chain_id": 80001,
//!            "creds": [{
//!              "mnemonic": { "seed": { "env": "MUMBAI_ACCOUNT_MNEMONIC" } }
//!            }]
//!          }
//!        }
//!      }
//!    },
//!    "current_network_profile": "dev"
//! }
//! # );
//! # let cfg: Config = from_str(&cfg_json.to_string()).unwrap();
//! ```
//!
//! You can load config files with [`Config::nearest`], which finds the JSON file in the current
//! directory or any parent directory:
//!
//! ```no_run
//! use cubist_config::Config;
//! let cfg = Config::nearest().unwrap();
//! ```
//! Alternatively, you can load the default config in the directory:
//!
//! ```no_run
//! use cubist_config::Config;
//! let cfg = Config::from_dir("/path/to/my-app").unwrap();
//! ```
//!
//! Alternatively, you can just use [`Config::from_file`] if you have the filename of the config
//! file.
//!
//! ```no_run
//! use cubist_config::Config;
//! let cfg = Config::from_file("/path/to/cubist-config.json").unwrap();
//! ```
//!
//! # Network Configuration
//!
//! Check the documentation for the [`network`] module for more details on:
//! - how to configure Cubist to automatically start local chains
//! - how to configure Cubist to use public testnets
//! - how to put a Cubist Proxy (to automatically handle transaction signing) in front of a public testnet
//! - how to pass secrets (e.g., an account mnemonic, or a URL containing a secret API key)

pub use network::{
    AvalancheConfig, CommonConfig, CredConfig, EndpointConfig, EthereumConfig, NetworkProfile,
    PolygonConfig, ProxyConfig,
};
use parse_display::{Display, FromStr};
use path_clean::PathClean;
use paths::Paths;
use schemars::JsonSchema;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::default::Default;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Bridge metadata
pub mod bridge;
/// Well-known paths
pub mod paths;
/// Manifest produced by the 'pre-compile' step
pub mod pre_compile_manifest;

/// This module exposes the Cubist network configuration interface [`NetworkProfile`].
///
/// A [`NetworkProfile`] provides configuration for each [`Target`] chain in use.
///
/// A portion of that configuration is common to all chains (see [`CommonConfig`]) and it includes:
///
/// - [`url`][CCUrl]: URL of the chain endpoint
/// - [`autostart`][CCautostart]: whether to start a local instance of the chain at `url`
///   (applies only if `url` is a localhost address)
/// - [`proxy`][CCproxy]: whether and how to start a Cubist Proxy in front or `url`
///   (applies only if `autostart` is false)
///
/// # Autostart Local Networks
///
/// If [`autostart`][CCAutostart] is true (default) and [`url`][CCUrl]
/// is a localhost address, Cubist (the `cubist start` command, to be
/// precise) will automatically start a local instance of the
/// specified chain.  For example,
///
/// ```
/// # use cubist_config::*;
/// # use serde_json::json;
/// # let np: NetworkProfile = serde_json::from_value(json!(
/// {
///   "ethereum": { "url": "http://localhost:8545", "autostart": true },
///   "polygon": { "url": "http://localhost:9545", "autostart": true }
/// }
/// # )).unwrap();
/// # assert!(np.ethereum.is_some());
/// # assert!(np.polygon.is_some());
/// # assert!(np.avalanche.is_none());
/// ```
///
/// will instruct Cubist to launch `anvil` (an Ethereum
/// implementation) and make its JSON RPC endpoint available at
/// `http://localhost:8545`, as well as `bor` (a Polygon
/// implementation) and make its JSON RPC endpoint available at
/// `http://localhost:9545`.
///
/// Cubist exposes different customization options for different
/// target chains: [EthereumConfig], [PolygonConfig], and
/// [AvalancheConfig].  Here are some examples:
///
/// ## Create funded accounts on Ethereum
///
/// The following configuration snippet will bootstrap `anvil` with 2 funded accounts generated
/// from a specific mnemonic.  The mnemonic is read from the `ETHEREUM_MNEMONIC` environment
/// variable (.env files are supported).
/// ```
/// # use cubist_config::*;
/// # use serde_json::json;
/// # let ec: EthereumConfig = serde_json::from_value(json!(
/// {
///   "url": "http://localhost:8545",
///   "autostart": true,
///   "bootstrap_mnemonic": {
///     "seed": { "env": "ETHEREUM_MNEMONIC" },
///     "account_count": 2
///   }
/// }
/// # )).unwrap();
/// ```
///
/// ## Create funded accounts on Polygon
///
/// The following configuration snippet will bootstrap `bor` with 2 funded accounts,
/// one generated from a given mnemonic (read from the `POLYGON_MNEMONIC` env var)
/// and one from a given private key (read from the `.polygon.secret` file).
/// ```
/// # use cubist_config::*;
/// # use serde_json::json;
/// # let pc: PolygonConfig = serde_json::from_value(json!(
/// {
///   "url": "http://localhost:8545",
///   "autostart": true,
///   "local_accounts": [
///     { "mnemonic":    { "seed": { "env": "POLYGON_MNEMONIC" } } },
///     { "private_key": { "hex":  { "file": ".polygon.secret" } } }
///    ]
/// }
/// # )).unwrap();
/// # assert_eq!(2, pc.local_accounts.len());
/// ```
///
/// # Connect to a Public Network
///
/// Before discussing how to configure Cubist to connect to public
/// networks, we must first introduce **Cubist Proxy**.
///
/// ## Cubist Proxy
///
/// Cubist Proxy is a component that sits between the client dApp and
/// an actual chain endpoint node and automatically signs every
/// transaction submitted via [eth_sendTransaction].  All other JSON
/// RPC requests (with a few exceptions) are forwarded to the real
/// endpoint (see [`CommonConfig::url`]) as is.
///
/// This is very useful when connecting to a public network, because
/// all credentials can be securely managed by Cubist
/// Proxy. Consequently, the client-side app doesn't have to worry
/// about managing secrets, signing, computing nonces, etc.
///
/// Proxy configuration is specified via the [`ProxyConfig`] struct
/// and it includes:
///
/// - [`port`](ProxyConfig::port): localhost port on which the proxy will be listening for requests
/// - [`chain_id`](ProxyConfig::chain_id): id of the target chain (so that Cubist Proxy can
///   automatically set it if not set by the client app)
/// - [`creds`](ProxyConfig::creds): credentials to use.
///
/// For every [eth_sendTransaction] request it receives, Cubist Proxy:
/// 1. extracts the `sender` field from it,
/// 1. looks up the credentials for that sender,
/// 1. populates any missing transaction parameters (e.g., chain id, nonce, etc.),
/// 1. signs the transaction on behalf of `sender` (using whatever signing mechanism is
///    configured for that sender in [`ProxyConfig::creds`]), and finally
/// 1. forwards the signed transaction to the endpoint via [eth_sendRawTransaction].
///
/// ## Example: Connecting to a Public Testnet
///
/// When using local chains only, no explicit account/authentication
/// configuration needs to be provided by the user.  That's because
/// Cubist will use a default account to start each local chain, and
/// Cubist Proxy will use that same account to automatically sign
/// transactions targeting that chain.
///
/// In contrast, when connecting to a public network, the user must
/// already have an account set up with that network provider.  If
/// that network only accepts signed transactions (and most likely it
/// does), Cubist must be configured to sign transactions using that
/// existing account.  Because the Cubist SDK does not deal with signing at all
/// (by design!), the solution is to configure and run a Cubist Proxy
/// in front of the remote endpoint.
///
/// The [`CommonConfig::url`] field is used to specify a public
/// network endpoint, e.g.,
/// `wss://polygon-mumbai.g.alchemy.com/v2/${{env.ALCHEMY_MUMBAI_API_KEY}}`[^api-key].
/// Additionally, Cubist Proxy must be configured with at least one
/// credential (see [`ProxyConfig::creds`]), which it will use to sign
/// all transactions.  The following example specifies a bip39
/// mnemonic; for other kinds of credentials, see [`CredConfig`].
///
/// ```
/// # use cubist_config::*;
/// # use serde_json::json;
/// # let cc: CommonConfig = serde_json::from_value(json!(
/// {
///   "url": "wss://polygon-mumbai.g.alchemy.com/v2/${{env.ALCHEMY_MUMBAI_API_KEY}}",
///   "proxy": {
///      "port": 9545,
///      "chain_id": 80001,
///      "creds": [{
///        "mnemonic": { "seed": { "env": "MUMBAI_ACCOUNT_MNEMONIC" } }
///      }]
///   }
/// }
/// # )).unwrap();
/// # assert!(cc.proxy.is_some());
/// # assert_eq!(Some(80001), cc.proxy.map(|p| p.chain_id));
/// ```
///
/// [CC]: crate::network::CommonConfig
/// [CCUrl]: crate::network::CommonConfig::url
/// [CCAutostart]: crate::network::CommonConfig::autostart
/// [CCProxy]: crate::network::CommonConfig::proxy
/// [eth_sendTransaction]: https://ethereum.org/en/developers/docs/apis/json-rpc/#eth_sendtransaction
/// [eth_sendRawTransaction]: https://ethereum.org/en/developers/docs/apis/json-rpc/#eth_sendrawtransaction
///
/// [^api-key]: Note the use of `${{env.ALCHEMY_MUMBAI_API_KEY}}`:
/// instead of hardcoding the secret API key into the plain-text
/// config file, we use the special `${{env.VAR_NAME}}` syntax to
/// instruct Cubist to read it from the environment.
pub mod network;

/// Interpolated string containing secrets
pub mod interpolation;
/// Secret management
pub mod secret;

/// Default cubist config filename
pub const DEFAULT_FILENAME: &str = "cubist-config.json";

/// Various utilities
pub mod util;

pub use pre_compile_manifest::FileArtifact;
pub use pre_compile_manifest::PreCompileManifest;

/// Errors raised handling configurations.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Error raised when we can't find the config file
    #[error("Could not find config file {}", DEFAULT_FILENAME)]
    FileNotFound,
    /// Error raised when deserialization/serialization of various config files fails.
    #[error("Malformed config {0}")]
    MalformedConfig(PathBuf, #[source] serde_json::Error),
    /// Error raised when contract files are not within root directory.
    #[error("Contract source files outside root directory: {0:?}")]
    InvalidContractFilePaths(Vec<PathBuf>),
    /// Generic path-related error, not caused by a filesystem error.
    #[error("{0}. Path: {1}")]
    PathError(&'static str, PathBuf),
    /// Generic filesystem error
    #[error("{0}. Path: {1}")]
    FsError(&'static str, PathBuf, #[source] std::io::Error),
    /// Generic IO error
    #[error("{0}")]
    IOError(&'static str, #[source] Option<std::io::Error>),
    /// Error raised when resolving globs fails.
    #[error("Glob error for pattern: {0}")]
    GlobError(String, #[source] Option<GlobErrorSource>),
    /// Error raised when a target has no matching files.
    #[error("No files found for target: {0:?}")]
    NoFilesForTarget(Target),
    /// Error raised when `network_profile` is specified but
    /// no such network profile is defined under `network_profiles`.
    #[error("Specified network profile ('{0}') not found")]
    MissingNetworkProfile(String),
    /// Error raised when secret cannot be ready from environment
    #[error("Failed to read secret from environment variable '{0}': {1}")]
    SecretReadFromEnv(String, #[source] dotenv::Error),
    /// Error raised when secret cannot be ready from file
    #[error("Failed to read secret from file '{0}': {1}")]
    SecretReadFromFile(PathBuf, #[source] std::io::Error),
    /// Error raised when interpolation produces an invalid URL    
    #[error("Invalid URL after applying interpolation.  Original URL: {0}")]
    UrlInterpolate(String),
    /// Error raised when provided URL scheme is invalid
    #[error("Invalid URL scheme: {0}.  Supported schemes are: ws, wss, http, https")]
    UrlInvalidScheme(String),
}

/// Various sources for glob errors
#[derive(Error, Debug)]
pub enum GlobErrorSource {
    /// Invalid pattern.
    #[error(transparent)]
    PatternError(#[from] glob::PatternError),
    /// Failure to execute glob pattern
    #[error(transparent)]
    GlobError(#[from] glob::GlobError),
}

/// Result with error type defaulting to [`ConfigError`].
pub type Result<T, E = ConfigError> = core::result::Result<T, E>;
/// Type alias for "network name" to be used in hash maps
pub type NetworkName = String;
/// Type alias for "network profile name" to be used in hash maps
pub type NetworkProfileName = String;
/// A source file that may contain contracts
pub type ContractFile = PathBuf;
/// Type alias for contract name.
pub type ContractName = String;
/// Type alias for function name
pub type FunctionName = String;
/// Type alias for event name
pub type EventName = String;
/// An object name (i.e., foo in uint256 foo)
pub type ObjectName = String;
/// A parameter name
pub type ParamName = String;

/// The project type. We support writing off-chain code in JavaScript, TypeScript, and Rust.
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema, Display, Debug)]
#[serde(deny_unknown_fields)]
#[derive(clap::ValueEnum)]
#[value(rename_all = "verbatim")]
pub enum ProjType {
    /// JavaScript
    JavaScript,
    /// TypeScript
    TypeScript,
    /// Rust
    Rust,
}

/// The compiler used for compiling contract code.
#[derive(Clone, Copy, PartialEq, Eq, FromStr, Deserialize, Serialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub enum Compiler {
    /// Compile with the solc compiler.
    #[serde(rename = "solc")]
    Solc,
    /// Compile with the solang compiler.
    #[serde(rename = "solang")]
    Solang,
}

impl Default for Compiler {
    fn default() -> Self {
        Compiler::Solc
    }
}

/// Target chains (e.g., Avalanche, Polygon, Ethereum) for which we can deploy contracts.
#[derive(PartialEq, Eq, Deserialize, Serialize, JsonSchema, Clone, Copy, Debug, Hash, Display)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "snake_case")]
#[display(style = "snake_case")]
pub enum Target {
    /// The avalanche chain
    Avalanche,
    /// The polygon chain
    Polygon,
    /// The ethereum chain
    Ethereum,
    /// The avalanche subnet chain
    AvaSubnet,
}

/// Targets are used as path segments all the time, so it's easiest just to canonicalize this here
impl AsRef<Path> for Target {
    fn as_ref(&self) -> &Path {
        match self {
            Target::Avalanche => "avalanche",
            Target::Polygon => "polygon",
            Target::Ethereum => "ethereum",
            Target::AvaSubnet => "ava_subnet",
        }
        .as_ref()
    }
}

/// Target configuration.
#[derive(PartialEq, Eq, Deserialize, Serialize, JsonSchema, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    /// List of globs pointing to source files.
    files: GlobsOrPaths,
    /// Compiler to compile the contract with.
    #[serde(default)]
    pub compiler: Compiler,
}

/// A list of globs or paths (always desrialzed as globs).
#[derive(PartialEq, Eq, Serialize, JsonSchema, Clone, Debug)]
#[serde(untagged)]
pub enum GlobsOrPaths {
    /// List of globs
    Globs(Vec<Glob>),
    /// List of resolved paths (i.e., globs that have been resolved to paths)
    Paths(Vec<PathBuf>),
}

/// We deserialize into globs.
impl<'de> Deserialize<'de> for GlobsOrPaths {
    fn deserialize<D>(deserializer: D) -> Result<GlobsOrPaths, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // always deserialize as a glob
        let globs = Vec::<Glob>::deserialize(deserializer)?;
        Ok(GlobsOrPaths::Globs(globs))
    }
}

impl TargetConfig {
    /// Get the list of source files for this target.
    pub fn files(&self) -> &Vec<PathBuf> {
        match &self.files {
            GlobsOrPaths::Paths(files) => files,
            _ => panic!("BUG: TargetConfig::files() called before TargetConfig::resolve_globs()"),
        }
    }

    /// Resolve the file globs for relative to config project directory.
    pub(crate) fn resolve_globs(&mut self, cfg: &Config) -> Result<()> {
        if let GlobsOrPaths::Globs(globs) = &self.files {
            self.files = GlobsOrPaths::Paths(self.resolve_globs_pure(globs, cfg)?);
        }
        Ok(())
    }

    fn resolve_globs_pure(&self, globs: &[Glob], cfg: &Config) -> Result<Vec<PathBuf>> {
        let root_dir = cfg.project_dir();
        let mut resolved_files = vec![];
        // Resolve file globs, keep track of all bad paths
        for Glob(glob) in globs.iter() {
            // Make the glob relative to the project root
            // (otherwise we're globbing relative to cwd())
            let rel_glob = root_dir.join(glob);
            let rel_glob_str = rel_glob.to_str().ok_or_else(|| {
                ConfigError::GlobError(format!("Invalid UTF-8 in glob '{:?}'", rel_glob), None)
            })?;

            let to_glob_error = |e| ConfigError::GlobError(rel_glob_str.into(), Some(e));

            let resolved_paths = glob::glob(rel_glob_str)
                .map_err(GlobErrorSource::PatternError)
                .map_err(to_glob_error)?
                .collect::<Result<Vec<PathBuf>, _>>()
                .map_err(GlobErrorSource::GlobError)
                .map_err(to_glob_error)?;
            for abs_path in resolved_paths {
                resolved_files.push(cfg.absolute_path_in_project(abs_path));
            }
        }

        Ok(resolved_files)
    }
}

/// A glob pattern for matching files.
#[derive(PartialEq, Eq, Deserialize, Serialize, JsonSchema, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Glob(String);

impl From<String> for Glob {
    fn from(s: String) -> Self {
        Glob(s)
    }
}

impl From<PathBuf> for Glob {
    fn from(p: PathBuf) -> Self {
        Glob(p.to_string_lossy().to_string())
    }
}

/// A map of chains to target configs
pub type TargetConfigs = HashMap<Target, TargetConfig>;

/// Contract configuration.
#[derive(PartialEq, Eq, Deserialize, Serialize, JsonSchema, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ContractsConfig {
    /// Root directory for contracts.
    pub root_dir: PathBuf,
    /// The map of chains to target configs.
    pub targets: TargetConfigs,
    /// Paths to search for imports.
    #[serde(default = "default_import_dirs")]
    pub import_dirs: Vec<PathBuf>,
}

impl Default for ContractsConfig {
    fn default() -> Self {
        ContractsConfig {
            root_dir: PathBuf::from("contracts"),
            targets: HashMap::new(),
            import_dirs: default_import_dirs(),
        }
    }
}

fn default_import_dirs() -> Vec<PathBuf> {
    vec!["node_modules".into()]
}

impl ContractsConfig {
    /// If `path` is under [`self.root_dir`], returns its relative path.
    pub fn relative_to_root(&self, path: &Path) -> Result<PathBuf> {
        path.strip_prefix(&self.root_dir)
            .map(|p| p.to_path_buf())
            .map_err(|_| ConfigError::InvalidContractFilePaths(vec![path.to_path_buf()]))
    }
}

/// Bridge provider options Cubist supports
#[derive(PartialEq, Eq, Deserialize, Serialize, JsonSchema, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub enum BridgeProvider {
    /// Use our bridging relayers
    Cubist,
    /// Use Axelar's interfaces and relayers
    Axelar,
}

/// Compiler configuration, i.e., configurations that result in compiler flags
#[derive(Clone, Default)]
pub struct CompilerConfig {
    /// Paths to search for imports.
    pub import_dirs: Vec<PathBuf>,
}

/// Top-level cubist application configuration.
///
/// Configs are consumed by all SDKs.
#[derive(Deserialize, Serialize, JsonSchema, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Absolute path to the file corresponding to this configuration.
    #[serde(skip_serializing, skip_deserializing)]
    pub config_path: PathBuf,
    /// Project type
    #[serde(rename = "type")]
    pub type_: ProjType,
    /// Path to the build directory.
    #[serde(default = "default_build_dir")]
    build_dir: PathBuf,
    /// Path to the deploy directory.
    #[serde(default = "default_deploy_dir")]
    deploy_dir: PathBuf,
    /// Contract configurations.
    #[serde(default)]
    contracts: ContractsConfig,
    /// A map of named network profiles for use in development, testing, etc.
    #[serde(default = "default_network_profiles")]
    pub network_profiles: HashMap<NetworkProfileName, NetworkProfile>,
    /// Selected network profile.  If omitted, defaults to "default". A
    /// network profile with the same name must be defined in `network_profiles`.
    #[serde(default = "default_profile_name")]
    pub current_network_profile: NetworkProfileName,
    /// The bridge provider to use for cross-chain invocations.
    #[serde(default = "default_bridge_provider")]
    pub bridge_provider: BridgeProvider,
    /// Allows or disables imports from external sources (GitHub and npm/Yarn).
    #[serde(default = "default_allow_import_from_external")]
    allow_import_from_external: bool,
}

fn default_build_dir() -> PathBuf {
    PathBuf::from("build")
}

fn default_deploy_dir() -> PathBuf {
    PathBuf::from("deploy")
}

fn default_profile_name() -> String {
    String::from("default")
}

fn default_network_profiles() -> HashMap<NetworkProfileName, NetworkProfile> {
    HashMap::from([(default_profile_name(), NetworkProfile::default())])
}

fn default_bridge_provider() -> BridgeProvider {
    BridgeProvider::Cubist
}

fn default_allow_import_from_external() -> bool {
    false
}

impl Default for Config {
    fn default() -> Self {
        Config {
            config_path: env::current_dir().unwrap().join(DEFAULT_FILENAME),
            type_: ProjType::JavaScript,
            build_dir: default_build_dir(),
            deploy_dir: default_deploy_dir(),
            contracts: Default::default(),
            network_profiles: default_network_profiles(),
            current_network_profile: default_profile_name(),
            bridge_provider: default_bridge_provider(),
            allow_import_from_external: default_allow_import_from_external(),
        }
    }
}

impl Config {
    /// Create configuration given type and directory.
    ///
    /// # Arguments
    ///
    /// * `type_`  - Project type
    /// * `dir` - Project directory
    ///
    /// # Example
    ///
    /// ```
    /// use cubist_config::{Config, ProjType};
    /// use tempfile::tempdir;
    /// use std::fs;
    /// # use pretty_assertions::assert_eq;
    ///
    /// // Create temp dir
    /// let dir = tempdir().unwrap();
    /// fs::create_dir_all(&dir).unwrap();
    ///
    /// // Create config (in temp dir)
    /// let cfg: Config = Config::new(ProjType::JavaScript, &dir).unwrap();
    ///
    /// assert_eq!(cfg.type_, ProjType::JavaScript);
    /// assert_eq!(cfg.build_dir(), dir.path().join("build"));
    /// assert_eq!(cfg.deploy_dir(), dir.path().join("deploy"));
    ///
    /// // Save config file to disk
    /// cfg.to_file(false).unwrap();
    ///
    /// // Load config from disk
    /// let cfg2 = Config::from_file(&cfg.config_path).unwrap();
    /// ```
    pub fn new(type_: ProjType, dir: impl AsRef<Path>) -> Result<Self> {
        let actual_dir = match fs::canonicalize(dir.as_ref()) {
            Ok(d) => Ok(d),
            Err(e) => Err(ConfigError::FsError(
                "Failed to cannonicalize path",
                dir.as_ref().into(),
                e,
            )),
        }?;
        let mut cfg = Config {
            config_path: actual_dir.join(DEFAULT_FILENAME),
            type_,
            ..Default::default()
        };
        // Resolved paths
        cfg.resolve_paths()?;
        // Validate config
        cfg.validate()?;
        Ok(cfg)
    }

    /// Get well-known paths
    pub fn paths(&self) -> Paths {
        Paths::new(self)
    }

    /// Create configuration from config file in the current directory or some parent directory.
    ///
    /// # Example
    ///
    /// ```
    /// use cubist_config::{Config, ProjType};
    /// use tempfile::tempdir;
    /// use std::fs;
    /// use std::env;
    ///
    /// // Create temp directory and chdir
    /// let dir = tempdir().unwrap();
    /// fs::create_dir_all(&dir).unwrap();
    /// assert!(env::set_current_dir(&dir).is_ok());
    ///
    /// // Create config
    /// let cfg: Config = Config::new(ProjType::JavaScript, ".").unwrap();
    /// cfg.to_file(false).unwrap();
    ///
    /// // Load config from disk
    /// let cfg2 = Config::nearest().unwrap();
    /// ```
    pub fn nearest() -> Result<Self> {
        match find_file(
            DEFAULT_FILENAME,
            env::current_dir().map_err(|e| {
                ConfigError::FsError("Failed to get current working directory", ".".into(), e)
            })?,
        ) {
            Ok(cfg_path) => Config::from_file(cfg_path),
            Err(err) => Err(err),
        }
    }

    /// Create configuration from directory (really from [`DEFAULT_FILENAME`] file in the directory).
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let cfg_path = dir.as_ref().join(DEFAULT_FILENAME);
        Config::from_file(cfg_path)
    }

    /// Create configuration from JSON file. Some paths can be overridden via environment
    /// variables:
    ///
    /// * Set `deploy_dir` via `CUBIST_DEPLOY_DIR`
    /// * Set `build_dir` via `CUBIST_BUILD_DIR`
    ///
    /// This function serves as the deserializer to all the other loaders (namely [`Self::nearest`]
    /// and [`Self::from_dir`]).
    pub fn from_file(config_path: impl AsRef<Path>) -> Result<Self> {
        let contents = match fs::read_to_string(config_path.as_ref()) {
            Ok(c) => Ok(c),
            Err(e) => Err(ConfigError::FsError(
                "Failed to read config file",
                config_path.as_ref().into(),
                e,
            )),
        }?;
        let mut app_cfg: Config = match serde_json::from_str(&contents) {
            Ok(c) => Ok(c),
            Err(e) => Err(ConfigError::MalformedConfig(config_path.as_ref().into(), e)),
        }?;
        // Set the config path
        app_cfg.config_path = match fs::canonicalize(config_path.as_ref()) {
            Ok(c) => Ok(c),
            Err(e) => Err(ConfigError::FsError(
                "Failed to canonicalize path",
                config_path.as_ref().into(),
                e,
            )),
        }?;
        // Potentially override paths via environment variables
        app_cfg.merge_paths_from_env();
        // Resolved paths
        app_cfg.resolve_paths()?;
        // Validate config
        app_cfg.validate()?;
        Ok(app_cfg)
    }

    /// Update config properties from environment variables.
    ///
    /// * Set `deploy_dir` via `CUBIST_DEPLOY_DIR`
    /// * Set `build_dir` via `CUBIST_BUILD_DIR`
    /// * Set `current_network_profile` via `CUBIST_NETWORK_PROFILE`
    fn merge_paths_from_env(&mut self) {
        if let Ok(deploy_dir) = env::var("CUBIST_DEPLOY_DIR") {
            tracing::debug!(
                "Setting deploy_dir from CUBIST_DEPLOY_DIR to {}",
                &deploy_dir
            );
            self.deploy_dir = PathBuf::from(deploy_dir);
        }
        if let Ok(build_dir) = env::var("CUBIST_BUILD_DIR") {
            tracing::debug!("Setting build_dir from CUBIST_BUILD_DIR to {}", &build_dir);
            self.build_dir = PathBuf::from(build_dir);
        }
        if let Ok(network_profile) = env::var("CUBIST_NETWORK_PROFILE") {
            tracing::debug!(
                "Setting current network profile from CUBIST_NETWORK_PROFILE to {}",
                &network_profile
            );
            self.current_network_profile = network_profile;
        }
    }

    /// Save configuration to new file.
    ///
    /// Writing to an existing file is generally discouraged (but can be done with by passing true
    /// fo the `force` argument). When we read configs from file, we resolve some paths (e.g., we
    /// turn contract globs into paths) and don't preserve the original. We also make the build,
    /// deploy, and contracts root dir relative to the project root. We do this largely because we
    /// use `to_file` when we create new projects. This does mean: `to_file(from_file(_))` is not
    /// the identiy function!
    pub fn to_file(&self, force: bool) -> Result<()> {
        if !force && self.config_path.is_file() {
            return Err(ConfigError::PathError(
                "Config file already exists",
                self.config_path.clone(),
            ));
        }
        let mut cfg = self.clone();
        // Make paths relative to project root
        cfg.build_dir = cfg.relative_to_project_dir(&cfg.build_dir)?;
        cfg.deploy_dir = cfg.relative_to_project_dir(&cfg.deploy_dir)?;
        cfg.contracts.root_dir = cfg.relative_to_project_dir(&cfg.contracts.root_dir)?;
        cfg.contracts.import_dirs = cfg
            .contracts
            .import_dirs
            .iter()
            .map(|d| cfg.relative_to_project_dir(d))
            .collect::<Result<_>>()?;

        let pretty = match serde_json::to_string_pretty(&cfg) {
            Ok(j) => Ok(j),
            Err(e) => Err(ConfigError::MalformedConfig(self.config_path.clone(), e)),
        }?;
        match fs::write(&self.config_path, pretty) {
            Ok(()) => Ok(()),
            Err(e) => Err(ConfigError::FsError(
                "Failed to write config to file",
                self.config_path.clone(),
                e,
            )),
        }
    }

    /// Get the top-level project directory
    pub fn project_dir(&self) -> PathBuf {
        self.config_path.parent().unwrap().to_path_buf()
    }

    /// Get the absolute deploy directory
    pub fn deploy_dir(&self) -> PathBuf {
        self.absolute_path_in_project(&self.deploy_dir)
    }

    /// Get the absolute build directory
    pub fn build_dir(&self) -> PathBuf {
        self.absolute_path_in_project(&self.build_dir)
    }

    /// Given a relative path, return absolute path prefixed by project root; otherwise return the
    /// absolute path.
    fn absolute_path_in_project(&self, path: impl AsRef<Path>) -> PathBuf {
        if path.as_ref().is_absolute() {
            path.as_ref().to_path_buf().clean()
        } else {
            self.project_dir().join(path).clean()
        }
    }

    /// Return relative path to the project root if the path is inside the project root.
    pub fn relative_to_project_dir(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = self.absolute_path_in_project(path);
        let project_dir = self.project_dir();
        match path.strip_prefix(&project_dir) {
            Ok(p) => Ok(p.to_path_buf()),
            Err(_) => Err(ConfigError::PathError(
                "Cannot make path relative to project root directory",
                path.to_path_buf(),
            )),
        }
    }

    /// Get contracts configurations.
    pub fn contracts(&self) -> &ContractsConfig {
        &self.contracts
    }

    /// Return all targets.
    pub fn targets(&self) -> impl Iterator<Item = Target> + '_ {
        self.contracts.targets.keys().copied()
    }

    /// Return configured network (if any) for a given target.
    pub fn network_for_target(&self, target: Target) -> Option<EndpointConfig> {
        self.network_for_target_in_profile(target, &self.current_network_profile)
    }

    /// Return configured network (if any) for a given target in a given profile.
    fn network_for_target_in_profile(
        &self,
        target: Target,
        profile_name: &NetworkProfileName,
    ) -> Option<EndpointConfig> {
        self.network_profiles
            .get(profile_name)
            .and_then(|profile| profile.get(target))
    }

    /// Return the currently selected network profile.
    pub fn network_profile(&self) -> &NetworkProfile {
        &self.network_profiles[&self.current_network_profile]
    }

    /// Check that the config is valid.
    fn validate(&self) -> Result<()> {
        // Validate contract paths
        self.validate_contract_paths()?;
        // Validate network profiles
        self.validate_network_profiles()
    }

    /// Resolve contracts file paths and ensure they're within the root directory.
    ///
    /// This resolves the contract directory and, for each target, the file globs.
    fn resolve_paths(&mut self) -> Result<()> {
        // Normalize contract root directory
        let mut contracts = self.contracts.clone();
        contracts.root_dir = self.absolute_path_in_project(&contracts.root_dir);
        contracts.import_dirs = contracts
            .import_dirs
            .iter()
            .map(|d| self.absolute_path_in_project(d))
            .collect();
        // Resolve file globs
        for (_, t_config) in contracts.targets.iter_mut() {
            t_config.resolve_globs(self)?;
        }
        // Update the config with the resolved contracts
        self.contracts = contracts;
        Ok(())
    }

    /// Validate contract paths. This ensures that all contract paths are within the root directory
    /// and that each target has at least one contract file.
    fn validate_contract_paths(&self) -> Result<()> {
        let contracts = &self.contracts;
        // Keep track of all bad paths
        let mut bad_paths = vec![];
        for (target, t_config) in contracts.targets.iter() {
            let files = t_config.files();
            if files.is_empty() {
                return Err(ConfigError::NoFilesForTarget(*target));
            }
            for file in files {
                if !file.starts_with(&contracts.root_dir) {
                    bad_paths.push(file.to_path_buf());
                }
            }
        }
        if !bad_paths.is_empty() {
            return Err(ConfigError::InvalidContractFilePaths(bad_paths));
        }
        Ok(())
    }

    /// Check if targets point to valid (defined) networks.
    fn validate_network_profiles(&self) -> Result<()> {
        // network profile is valid
        if self
            .network_profiles
            .get(&self.current_network_profile)
            .is_none()
        {
            return Err(ConfigError::MissingNetworkProfile(
                self.current_network_profile.clone(),
            ));
        }

        Ok(())
    }

    /// Returns `true` if imports from external sources are allowed.
    pub fn allow_import_from_external(&self) -> bool {
        self.allow_import_from_external
    }

    /// Returns the compiler configuration for this Cubist configuration.
    pub fn get_compiler_config(&self) -> CompilerConfig {
        let abs_import_dirs = self
            .contracts
            .import_dirs
            .iter()
            .map(|import_dir| self.absolute_path_in_project(import_dir))
            .collect::<Vec<_>>();
        CompilerConfig {
            import_dirs: abs_import_dirs,
        }
    }
}

/// Find file starting from directory.
///
/// # Arguments
///
/// * `file` - Config filename
/// * `dir`  - Starting directory
///
/// # Errors
///
/// Fails with [`ConfigError::FileNotFound`] if we cannot find the file.
fn find_file(file: impl AsRef<Path>, dir: impl AsRef<Path>) -> Result<PathBuf> {
    let mut path: PathBuf = PathBuf::from(dir.as_ref());

    loop {
        let candidate = path.join(file.as_ref());
        if candidate.is_file() {
            break Ok(candidate);
        }
        if !path.pop() {
            break Err(ConfigError::FileNotFound);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn should_find_file() {
        let tmp = tempdir().unwrap();
        let file_path = tmp.path().join("a.foo");

        fs::write(&file_path, "{}").unwrap();
        let found_file = find_file("a.foo", &tmp).unwrap();
        assert_eq!(found_file, file_path);
    }

    #[test]
    fn should_not_find_file() {
        let tmp = tempdir().unwrap();

        match find_file("a.foo", &tmp) {
            Err(ConfigError::FileNotFound) => {}
            _ => panic!("Found file that should not exist"),
        };
    }

    #[test]
    fn test_new_config() {
        let tmp = tempdir().unwrap();
        let file_path = tmp.path().join(DEFAULT_FILENAME);

        // create contracts/{ava,poly}.sol
        let contracts_dir = tmp.path().join("contracts");
        fs::create_dir(&contracts_dir).unwrap();
        fs::write(contracts_dir.join("ava.sol"), "contract A {}").unwrap();
        fs::write(contracts_dir.join("poly.sol"), "contract P {}").unwrap();

        fs::write(
            &file_path,
            r#"
            {
              "type": "JavaScript",
              "build_dir": "./build_dir",
              "deploy_dir": "../deploy_dir",
              "contracts": {
                 "root_dir": "./contracts",
                 "targets": {
                   "avalanche": { "files": ["./contracts/ava.sol"] },
                   "polygon": { "files": ["./contracts/poly.sol"] }
                 }
              },
              "network_profiles": {
                 "default": {
                    "avalanche": { "url": "http://otherhost:9560" },
                    "polygon": { "url": "http://localhost:9545" },
                    "ethereum": { "url": "http://localhost:7545" }
                }
              }
            }
        "#,
        )
        .unwrap();

        // create config
        let cfg = Config::from_dir(&tmp).unwrap();
        assert_eq!(cfg.type_, ProjType::JavaScript);
        assert_eq!(cfg.build_dir(), tmp.path().join("build_dir").clean());
        assert_eq!(cfg.deploy_dir(), tmp.path().join("../deploy_dir").clean());
        assert_eq!(false, cfg.allow_import_from_external);
        let contracts = cfg.contracts();
        assert_eq!(contracts.root_dir, tmp.path().join("contracts").clean());
        assert_eq!(
            contracts.targets.get(&Target::Avalanche).unwrap().files(),
            &vec![tmp.path().join("contracts/ava.sol").clean()]
        );
        assert_eq!(
            contracts.targets.get(&Target::Polygon).unwrap().files(),
            &vec![tmp.path().join("contracts/poly.sol").clean()]
        );
        assert_eq!(
            contracts.import_dirs,
            vec![tmp.path().join("node_modules").clean()]
        );
        let profile = &cfg.network_profiles["default"];
        assert_eq!(
            profile.avalanche.as_ref().unwrap().common.url.to_string(),
            "http://otherhost:9560/"
        );
        assert_eq!(
            profile.polygon.as_ref().unwrap().common.url.to_string(),
            "http://localhost:9545/"
        );
        assert_eq!(
            profile.ethereum.as_ref().unwrap().common.url.to_string(),
            "http://localhost:7545/"
        );
    }

    #[test]
    fn test_bad_contract_paths_outside() {
        // this test ensures that we catch contracts that are outisde the contract root dir
        let tmp = tempdir().unwrap();

        // create contracts directory in tmp
        let outside_contracts_dir = tmp.path().join("outside_contracts");
        fs::create_dir(&outside_contracts_dir).unwrap();
        fs::write(outside_contracts_dir.join("ava.sol"), "contract A {}").unwrap();
        fs::write(outside_contracts_dir.join("poly.sol"), "contract P {}").unwrap();

        // create app dir
        let app_root = tmp.path().join("app");
        fs::create_dir(&app_root).unwrap();
        // create actual contracts directory containing eth.sol
        let contracts_dir = app_root.join("contracts");
        fs::create_dir(&contracts_dir).unwrap();
        fs::write(contracts_dir.join("ava0.sol"), "contract A {}").unwrap();
        fs::write(contracts_dir.join("poly0.sol"), "contract P {}").unwrap();
        fs::write(contracts_dir.join("eth.sol"), "contract E {}").unwrap();

        let file_path = app_root.join(DEFAULT_FILENAME);
        let cfg = serde_json::json!({
            "type": "JavaScript",
            "contracts": {
                "root_dir": "./contracts",
                "targets": {
                    "avalanche": { "files": [ "../outside_contracts/ava.sol",
                                              "./contracts/ava0.sol" ] },
                    "polygon": { "files": [ outside_contracts_dir.join("poly.sol"),
                                            "./contracts/poly0.sol" ] },
                    "ethereum": { "files": ["./contracts/../contracts/eth.sol"] }
                }
            }
        });
        fs::write(&file_path, cfg.to_string()).unwrap();

        // create config
        match Config::from_dir(&app_root) {
            Err(ConfigError::InvalidContractFilePaths(mut paths)) => {
                assert_eq!(
                    paths.sort(),
                    vec![
                        app_root.join("../contracts/ava.sol").clean(),
                        outside_contracts_dir.join("poly.sol").clean(),
                    ]
                    .sort()
                );
            }
            c => panic!("Expected error, but got: {:?}", c),
        }
    }

    #[test]
    fn test_bad_contract_paths_empty() {
        // this test ensures that we catch cases where the config glob points to no actual files
        let tmp = tempdir().unwrap();

        // create app dir
        let app_root = tmp.path().join("app");
        fs::create_dir(&app_root).unwrap();
        // create actual contracts directory containing eth.sol
        let contracts_dir = app_root.join("contracts");
        fs::create_dir(&contracts_dir).unwrap();
        fs::write(contracts_dir.join("ava.sol"), "contract A {}").unwrap();
        fs::write(contracts_dir.join("poly.sol"), "contract P {}").unwrap();

        let file_path = app_root.join(DEFAULT_FILENAME);
        let cfg = serde_json::json!({
            "type": "JavaScript",
            "contracts": {
                "root_dir": "./contracts",
                "targets": {
                    "avalanche": { "files": [ "../**/ava.sol", ] },
                    "polygon": { "files": [ "./contracts/poly.sol" ] },
                    "ethereum": { "files": ["./contracts/eth_not_real.sol"] }
                }
            }
        });
        fs::write(&file_path, cfg.to_string()).unwrap();

        // create config
        match Config::from_dir(&app_root) {
            Err(ConfigError::NoFilesForTarget(Target::Ethereum)) => (),
            c => panic!("Expected error, but got: {:?}", c),
        }
    }

    #[test]
    fn test_defaults() {
        let tmp = tempdir().unwrap();
        let file_path = tmp.path().join(DEFAULT_FILENAME);

        fs::write(&file_path, r#" { "type": "JavaScript" } "#).unwrap();

        // change current directory
        let cfg = Config::from_dir(&tmp).unwrap();
        assert_eq!(cfg.type_, ProjType::JavaScript);
        assert_eq!(cfg.build_dir(), tmp.path().join("build").clean());
        assert_eq!(cfg.deploy_dir(), tmp.path().join("deploy").clean());
        let contracts = cfg.contracts();
        assert_eq!(contracts.root_dir, tmp.path().join("contracts").clean());
        assert!(contracts.targets.is_empty());
        let network_profiles = cfg.network_profile();
        assert!(network_profiles.ethereum.is_none());
        assert!(network_profiles.polygon.is_none());
        assert!(network_profiles.avalanche.is_none());
        assert!(network_profiles.ava_subnet.is_none());
    }

    #[test]
    fn test_from_dir() {
        let tmp = tempdir().unwrap();
        let file_path = tmp.path().join(DEFAULT_FILENAME);

        fs::write(&file_path, r#" { "type": "Rust" } "#).unwrap();

        // create config
        let cfg = Config::from_dir(tmp.path()).unwrap();
        assert_eq!(cfg.type_, ProjType::Rust);
        assert_eq!(cfg.build_dir(), tmp.path().join("build").clean());
        assert_eq!(cfg.deploy_dir(), tmp.path().join("deploy").clean());
        let contracts = cfg.contracts();
        assert_eq!(contracts.root_dir, tmp.path().join("contracts").clean());
        assert!(contracts.targets.is_empty());
    }

    #[test]
    fn test_deny_unknown_fields() {
        let tmp = tempdir().unwrap();
        let file_path = tmp.path().join(DEFAULT_FILENAME);

        // create config with bad field
        fs::write(
            &file_path,
            r#" { "type": "JavaScript", "bad-field": false } "#,
        )
        .unwrap();

        // create config
        let cfg = Config::from_dir(&tmp);
        match cfg {
            Err(ConfigError::MalformedConfig(..)) => {}
            _ => panic!("Should have failed to parse"),
        }
    }

    #[test]
    fn test_fail_on_bogus_dir() {
        let tmp = tempdir().unwrap();
        match Config::new(ProjType::JavaScript, tmp.path().join(":")) {
            Err(ConfigError::FsError(..)) => {}
            _ => panic!("Should have failed"),
        }
    }

    #[test]
    fn bogus_profile_name() {
        let tmp = tempdir().unwrap();
        let file_path = tmp.path().join(DEFAULT_FILENAME);

        fs::write(
            &file_path,
            r#"
            {
              "type": "JavaScript",
              "build_dir": "./build_dir",
              "deploy_dir": "../deploy_dir",
              "contracts": {
                 "root_dir": "./contracts",
                 "targets": {}
              },
              "network_profiles": {
                 "default": {},
                 "other_profile": {}
              },
              "current_network_profile": "bogus_profile"
            }
        "#,
        )
        .unwrap();

        let maybe_cfg = Config::from_dir(tmp);
        assert!(maybe_cfg.is_err());
        match maybe_cfg.unwrap_err() {
            ConfigError::MissingNetworkProfile(bogus_profile_name) => {
                assert_eq!("bogus_profile", bogus_profile_name)
            }
            err => panic!("Expected `MissingNetworkProfile` error, got: {:?}", err),
        }
    }

    #[test]
    fn test_multiple_profiles() {
        let tmp = tempdir().unwrap();
        let file_path = tmp.path().join(DEFAULT_FILENAME);

        fs::write(
            &file_path,
            r#"
            {
              "type": "JavaScript",
              "build_dir": "./build_dir",
              "deploy_dir": "../deploy_dir",
              "contracts": {
                 "root_dir": "./contracts",
                 "targets": {}
              },
              "network_profiles": {
                 "profile1": {
                    "avalanche": { "url": "http://localhost:1000" },
                    "polygon": { "url": "http://localhost:3000" }
                },
                 "profile2": {
                    "avalanche": { "url": "http://localhost:2000" },
                    "polygon": { "url": "http://localhost:3000" }
                }
              },
              "current_network_profile": "profile2"
            }
        "#,
        )
        .unwrap();

        let cfg = Config::from_dir(tmp).unwrap();
        let profile1 = cfg.network_profiles["profile1"].clone();
        let profile2 = cfg.network_profiles["profile2"].clone();
        // profile1
        assert_eq!(1000, profile1.avalanche.unwrap().common.url.port().unwrap());
        assert_eq!(3000, profile1.polygon.unwrap().common.url.port().unwrap());
        // profile2
        assert_eq!(2000, profile2.avalanche.unwrap().common.url.port().unwrap());
        assert_eq!(3000, profile2.polygon.unwrap().common.url.port().unwrap());
    }
}
