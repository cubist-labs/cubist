#![warn(missing_docs)]
#![doc(html_no_source)]

//! SDK for working with [Cubist] projects.
//!
//! # Background
//!
//! Cubist makes it easy to develop **cross-chain** dApps by making them
//! look and feel like single-chain dApps.
//!
//! A user writes contracts as if they were all going to be deployed
//! on the same chain.  For example, one contract may directly call a
//! method on another contract, as if they were on the same chain
//! (even though they will not be).
//!
//! Next, the user provides a [configuration][CubistConfig] file,
//! where, among other things, the user assigns each contract source
//! file to a single chain.
//!
//! Finally, to compile such a cross-chain dApp, for each chain from
//! the config file Cubist generates (behind the scenes) a separate
//! "target project", i.e., a standard single-chain project amenable
//! to existing web3 tooling.  To facilitate cross-chain
//! interactions[^xchain-deps] between contracts, however, Cubist
//! generates (again, behind the scenes, without requiring any user
//! interaction) a **shim** contract for every cross-chain callee
//! contract and places it in the target project for the chain on
//! which that contract is called[^relayer].
//!
//! # Example
//!
//! Consider the following extremely simple multi-chain storage dApp
//! consisting of two contracts:
//! - `Receiver`, which exposes a simple interface for storing a number,
//!     ```solidity
//!     contract Receiver {
//!       uint256 _number;
//!     
//!       function store(uint256 num) public {
//!         _number = num;
//!       }
//!
//!       function retrieve() public view returns (uint256) {
//!         return _number;
//!       }
//!     }
//!     ```
//! - `Sender`, which exposes the same interface but it stores a given number
//! in two contracts: itself and an instance of `Receiver`.
//!     ```solidity
//!     import './Receiver.sol';
//!     
//!     contract Sender {
//!       Receiver _receiver;
//!       uint256 _number;
//!     
//!       constructor (Receiver addr) {
//!         _receiver = addr;
//!       }
//!     
//!       function store(uint256 num) public {
//!         _number = num;
//!         _receiver.store(_number);
//!       }
//!
//!       function retrieve() public view returns (uint256) {
//!         return _number;
//!       }
//!     }
//!     ```
//!
//! Let's also assume that we want the `Receiver` contract deployed on
//! [Ethereum] and the `Sender` contract deployed on
//! [Polygon][^ref-config].
//!
//! When instructed to build this dApp (e.g., by a user running
//! `cubist build` from the command line), Cubist generates two target
//! projects, one for each chain:
//!
//! - the [Ethereum] project contains only the `Receiver` contract
//! (unchanged),
//!
//! - the [Polygon] project contains the `Sender` contract (unchanged)
//! as well as an automatically generated `Receiver` shim contract;
//! the shim contract has exactly the same interface as the original
//! receiver contract (so that `Sender` can remain unchanged).  The
//! key difference, however, is that the shim contract's `store`
//! method now only generates an event (containing the method argument
//! in its field).  This event is automatically picked up by the
//! relayer and relayed to the original `Receiver` contract deployed
//! on [Ethereum].
//!
//! Once Cubist has created the shims in each target project, it
//! individually builds each target project using a native contract
//! compiler (currently, `solc` is the only supported compiler for
//! contracts written in [Solidity]).
//!
//! Once the contracts are compiled, we still need to write an app to
//! interact with them (e.g., deploy them, invoke methods, run tests,
//! etc.). For apps written in Rust, this SDK provides the necessary
//! abstractions.
//!
//! # API
//!
//! This crate exposes a number of abstractions for interacting with a
//! Cubist project:
//!
//! - [CubistInfo] contains metadata about a Cubist dApp, e.g.,
//!   - which contracts target which chains (e.g., `Receiver` targets [Ethereum]
//!     and `Sender` targets [Polygon]),
//!   - which contracts have shims on which chains (e.g., only `Receiver` has a shim on [Polygon])
//!   - general project [configuration][CubistConfig], etc.
//!
//! - [TargetProjectInfo] contains metadata about a single-chain target project, e.g.,
//!   its target chain, compiler settings, chain endpoint configuration, etc.
//!
//! - [ContractInfo] contains metadata about a single contract, e.g., full name,
//!   source code information, ABI, and compiled bytecode.
//!
//! To communicate and interact with an actual on-chain endpoint node, those
//! abstractions must first be instantiated with a concrete middleware
//! (either [Http] or [Ws]), i.e.,
//! - [`Cubist<M>`] is a grouping of instantiated target projects and
//!   contracts.  Within this grouping, each contract may be deployed
//!   at most once (i.e., [`Contract<M>::deploy`] is idempotent).  When
//!   multiple deployments per contract are needed, multiple [`Cubist<M>`]
//!   instances may be created within the same app.
//!
//! - [`TargetProject<M>`] is a wrapper around [TargetProjectInfo] which
//!   additionally contains an [ethers provider] used to talk to the
//!   chain endpoint
//!
//! - [`Contract<M>`] is a wrapper around [`ContractInfo`] which can additionally
//!   be deployed; once deployed, its methods may be called via [send][contract-send]
//!   or [call][contract-call][^deploy-factory]
//!
//! # API Examples
//!
//! ## Instantiate [`Cubist<M>`] for a given dApp
//!
//! ```
//! use cubist_sdk::*;
//! use cubist_config::Config;
//!
//! async {
//!   // expects to find 'cubist-config.json' in any of the parent folders
//!   let cfg = || Config::nearest().expect("cubist-config.json not found");
//!
//!   // create a Cubist instance over HTTP
//!   let cubist_http = Cubist::<Http>::new(cfg()).await.unwrap();
//!
//!   // create a cubist instance over WebSockets
//!   let cubist_ws = Cubist::<Ws>::new(cfg()).await.unwrap();
//!
//!   // create an un-instantiated (not connected to the endpoint) 'CubistInfo' instance
//!   let cubist = CubistInfo::new(cfg()).unwrap();
//! };
//! ```
//!
//! ## Deploy `Sender` and `Receiver` contracts then call `store`
//!
//! ```
//! use cubist_sdk::*;
//! use cubist_config::Config;
//! use ethers::types::U256;
//!
//! async {
//!   // expects to find 'cubist-config.json' in any of the parent folders
//!   let cfg = || Config::nearest().expect("cubist-config.json not found");
//!
//!   // create a Cubist instance over HTTP
//!   let cubist = Cubist::<Http>::new(cfg()).await.unwrap();
//!
//!   // find contracts by their names
//!   let receiver = cubist.contract("Receiver").expect("Contract 'Receiver' not found");
//!   let sender = cubist.contract("Sender").expect("Contract 'Sender' not found");
//!
//!   // deploy first 'Receiver' then 'Sender'
//!   receiver.deploy(()).await.unwrap();
//!   sender.deploy(receiver.address_on(sender.target())).await.unwrap();
//!
//!   // call 'store' on the sender
//!   let val = U256::from(123);
//!   sender.send("store", val).await.unwrap();
//!
//!   // call 'retrieve' on both 'Sender' and 'Receiver'
//!   assert_eq!(val, sender.call("retrieve", ()).await.unwrap());
//!
//!   // if the relayer is running, it will automatically propagate the value to 'Receiver'
//!   tokio::time::sleep(std::time::Duration::from_millis(100)).await;
//!   assert_eq!(val, receiver.call("retrieve", ()).await.unwrap());
//! };
//! ```
//!
//! ## Load already deployed contracts from existing deployment receipts
//! ```
//! use cubist_sdk::*;
//! use cubist_config::Config;
//! use ethers::types::U256;
//!
//! async {
//!   // expects to find 'cubist-config.json' in any of the parent folders
//!   let cfg = || Config::nearest().expect("cubist-config.json not found");
//!
//!   // create a Cubist instance over HTTP
//!   let cubist = Cubist::<Http>::new(cfg()).await.unwrap();
//!
//!   // find contracts by their names
//!   let receiver = cubist.contract("Receiver").expect("Contract 'Receiver' not found");
//!   let sender = cubist.contract("Sender").expect("Contract 'Sender' not found");
//!
//!   // if 'Receiver' and 'Sender' were previously deployed using Cubist, and the deployment
//!   // receipts are still on disk (in 'deploy' directory, by default), we can reload just them
//!   receiver.deployed().await.unwrap();
//!   sender.deployed().await.unwrap();
//!
//!   // call 'store' on the sender
//!   let val = U256::from(123);
//!   sender.send("store", val).await.unwrap();
//!
//!   // call 'retrieve' on both 'Sender' and 'Receiver'
//!   assert_eq!(val, sender.call("retrieve", ()).await.unwrap());
//!
//!   // if the relayer is running, it will automatically propagate the value to 'Receiver'
//!   tokio::time::sleep(std::time::Duration::from_millis(100)).await;
//!   assert_eq!(val, receiver.call("retrieve", ()).await.unwrap());
//! };
//! ```
//!
//! [Cubist]: https://cubist.dev
//! [Solidity]: https://soliditylang.org/
//! [Ethereum]: cubist_config::Target::Ethereum
//! [Polygon]: cubist_config::Target::Polygon
//! [CubistConfig]: cubist_config::Config
//! [contract-send]: Contract<M>::send
//! [contract-call]: Contract<M>::call
//! [contract-deploy]: Contract<M>::deploy
//! [ethers provider]: ethers::providers::Provider<M>
//!
//! [^xchain-deps]: Cubist automatically discovers all cross-chain
//! dependencies by statically analyzing the contract source files.
//!
//! [^relayer]: A separate component, called **relayer**, continuously
//! monitors events triggered by shim contracts and automatically
//! relays them to their final destinations.
//!
//! [^ref-config]: Refer to [cubist_config::Config] for details on how
//! to configure a Cubist dApp and assign contracts to different
//! chains.
//!
//! [^deploy-factory]: The current [`Contract<M>`] API is somewhat weird
//! in that it is stateful, i.e., that some methods (like
//! [send][contract-send] and [call][contract-call]) may only be
//! called after [deploy][contract-deploy].  This is subject to
//! change! The [`Contract<M>`] type will likely be decoupled into
//! "ContractFactory" and "Contract".

use crate::core::{HttpStack, WsStack};
use cubist_config::secret::SecretUrl;
use cubist_config::{paths::ContractFQN, ConfigError, Target};
use ethers::types::ParseBytesError;
use solang_parser::diagnostics::Diagnostic;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Contract and project management data structures.
pub mod core;
/// The interface (shim) contract generator.
pub mod gen;
/// Utilities for parsing contract files
pub mod parse;

/// Module for analyzing contracts.
mod analyze;
/// Handlers for different compilers (e.g., solc, solang, etc.)
mod target_handler;

/// Type alias for middleware stack over HTTP
pub type Http = HttpStack;
/// Type alias for middleware stack over Web Sockets
pub type Ws = WsStack;

/// Re-export some very commonly used types.
pub use crate::core::{
    Contract, ContractInfo, Cubist, CubistInfo, TargetProject, TargetProjectInfo,
};

/// Custom error type wrapping various third-party errors
#[allow(missing_docs)]
#[derive(Debug, Error)]
pub enum WrapperError {
    #[error(transparent)]
    SolcError(#[from] ethers_solc::error::SolcError),
    #[error("IO error for path {0}")]
    IOError(PathBuf, #[source] std::io::Error),
    #[error("Error deserializing JSON from file '{0}' into '{1}' type")]
    JsonError(PathBuf, String, #[source] serde_json::Error),
    #[error(transparent)]
    UrlError(#[from] url::ParseError),
    #[error(transparent)]
    AbiError(#[from] ethers::abi::Error),
    #[error(transparent)]
    ContractAbiError(#[from] ethers::contract::AbiError),
    #[error("{0}")]
    ContractError(String),
    #[error("ProviderError when calling '{0}': {1}")]
    ProviderError(String, String),
    #[error(transparent)]
    ParseBytesError(#[from] ParseBytesError),
    #[error(transparent)]
    ConfigError(#[from] cubist_config::ConfigError),
}

/// Errors raised by this crate.
#[derive(Debug, Error)]
pub enum CubistSdkError {
    /// Error raised when a requested target is not found in the Cubist config.
    #[error("Target {0} not found in Cubist config")]
    MissingTarget(Target),
    /// Error raised when no compilation manifest is found.
    #[error(
        "Error reading compilation manifest for target '{0}'. Did you run 'cubist pre-compile'?"
    )]
    MissingManifest(Target, #[source] ConfigError),
    /// Error raised when network configuration is missing for target
    #[error("Network configuration missing for target '{0}'")]
    MissingNetworkConfig(Target),
    /// Error raised when a compiled contract cannot be loaded.
    #[error("Failed to parse contract artifact {0}.\nReason: {1}")]
    ParseContractError(PathBuf, String, #[source] Option<WrapperError>),
    /// Error raised when artifacts directory cannot be read.
    #[error("Failed to read artifacts directory {0}.\nDid you run 'cubist build'?")]
    NoArtifactsDir(PathBuf, #[source] WrapperError),
    /// Error raised when loading bridge file fails
    #[error("Failed to load bridge for contract {0} (from bridge file {1})")]
    LoadBridgeError(ContractFQN, PathBuf, #[source] WrapperError),
    /// Error raised when a compilation fails.
    #[error("Error compiling {0}: {1}")]
    CompileError(PathBuf, String, #[source] Option<WrapperError>),
    /// Error raised when cleaning build directory fails
    #[error("Error cleaning project build directory {0}")]
    CleanError(PathBuf, #[source] WrapperError),
    /// Error raised when creating a deployer fails.
    #[error("Failed to connect to target '{0}' at {1}")]
    CreateClientError(Target, SecretUrl, #[source] Option<WrapperError>),
    /// Error raised when deployment fails.
    #[error("Error deploying contract '{0}' to {1}. Reason: {2}")]
    DeployError(ContractFQN, Target, String),
    /// Error raised when a contract method call fails.
    #[error("Error calling '{method_name}' on contract '{contract}' on chain '{target}'")]
    CallError {
        /// Contract name,
        contract: ContractFQN,
        /// Target chain.
        target: Target,
        /// Method that could not be called.
        method_name: String,
        /// Error message
        #[source]
        source: WrapperError,
    },
    /// Error raised when updating a shim contract's access control fails.
    #[error("Failed to add '{caller_contract}' to approved callers of shim '{shim_contract}' on chain '{target}'")]
    ApproveCallerError {
        /// Native contract that had to be added to approved callers for the shim contract below
        caller_contract: ContractFQN,
        /// Shim contract whose access control had to be updated
        shim_contract: ContractFQN,
        /// Target chain
        target: Target,
    },
    /// Error raised when a contract call is attempted before the contract has been deployed.
    #[error("Contract '{contract}' not yet deployed to '{target}'")]
    ContractNotDeployed {
        /// Contract name,
        contract: ContractFQN,
        /// Target chain.
        target: Target,
    },
    /// Error raised when saving deployment manifest fails
    #[error("Failed to save deployment manifest to file {0}")]
    SaveDeploymentManifestError(PathBuf, #[source] WrapperError),
    /// Error raised when saving deployment receipt fails
    #[error("Failed to save deployment receipt to file {0}")]
    SaveDeploymentReceiptError(PathBuf, #[source] WrapperError),
    /// Error raised when saving deployment receipt fails
    #[error("Failed to deserialize deployment receipt from file {0}")]
    DeserializeDeploymentReceiptError(PathBuf, #[source] WrapperError),
    /// A generic error when reading a file
    #[error("Error reading file {0}")]
    ReadFileError(PathBuf, #[source] io::Error),
    /// Failed to parse contract file
    #[error("Error parsing file {0}: {1:#?}")]
    ParseError(PathBuf, Vec<Diagnostic>),
    /// A file contained a unicode import
    #[error("Unsupported unicode import {0} in file {1}")]
    UnicodeImportError(String, PathBuf),
    /// An absolute path import points into the contracts root directory.
    /// This is disallowed because Cubist later moves the code in the contracts root.
    #[error("Import of absolute path {0} (in file {1}) pointing into contract root directory. Please use relative paths for imports pointing into the contract root dir.")]
    AbsolutePathError(String, PathBuf),
    /// An imported relative path points outside the contracts root, and exists in a file
    /// within the contracts root. This is disallowed once again because Cubist moves the
    /// code in the contracts root.
    #[error("Import of relative path {0} from outside contracts root directory, from file {1} within contract root directory. Files in contract root are copied, which will break this relative import.")]
    RelativePathError(String, PathBuf),
    /// Couldn't canonicalize paths needed for checking for absolute and relative path errors
    #[error("Unable to canonicalize relative import {0} in file {1}")]
    CanonicalizationError(String, PathBuf, #[source] Option<std::io::Error>),
    /// Error raised when retrieving accounts from the chain provider fails
    #[error("Failed to retrieve accounts for chain '{0}': {1}")]
    AccountsError(Target, String),
    /// Error raised when trying to load a contract when no deployment receipts are found for it.
    #[error("No deployment receipts found for contract '{0}' on target {1}")]
    LoadContractNoReceipts(ContractFQN, Target),
    /// Error raised when trying to load a contract when more than one deployment receipt is found for it.
    #[error("More than one deployment receipt found for contract '{0}' on target {1}")]
    LoadContractTooManyReceipts(ContractFQN, Target),
    /// Error forwarded from Cubist localchains
    #[error(transparent)]
    LocalChainsError(#[from] cubist_localchains::error::Error),
    /// Error forwarded from Cubist config
    #[error(transparent)]
    ConfigError(#[from] cubist_config::ConfigError),
}

/// Result with error type defaulting to [`CubistSdkError`].
pub type Result<T, E = CubistSdkError> = ::core::result::Result<T, E>;
