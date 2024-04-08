use crate::core::DeploymentInfo;
use crate::core::DeploymentManifest;
use crate::gen::backend;
use crate::gen::APPROVE_CALLER_METHOD_NAME;
use crate::CubistSdkError;
use crate::Result;
use crate::WrapperError;
use cubist_config::paths::{hex, ContractFQN};
use cubist_config::util::OrBug;
use cubist_config::Target;
use ethers::abi::{Address, Detokenize, Tokenize};
use ethers::core::abi::Abi;
use ethers::prelude::builders::ContractCall;
use ethers::providers::Middleware;
use ethers::types::{Bytes, TransactionReceipt};
use futures::FutureExt;
use soroban_env_host::xdr::ScSpecEntry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;
use tracing::Level;
use tracing::{debug, span, trace, warn};

use super::project::BridgeInfo;
use super::{HttpStack, TargetProject};

type EthersContract<M> = ethers::contract::Contract<M>;

/// Data for the contract
pub enum ContractData {
    /// Data related to a Solidity contract
    SolidityData {
        /// Contract abi
        abi: Abi,
        /// Contract bytecode
        bytes: Bytes,
    },
    /// Data related to a Soroban contract
    SorobanData {
        /// Path of the Wasm file
        wasm_path: PathBuf,
        /// Spec of the Soroban contracts
        spec_entries: Vec<ScSpecEntry>,
        /// Hash of the Wasm code
        hash: String,
    },
}

/// Contract compiled into abi and bytecode.
pub struct ContractInfo {
    /// Fully qualified name
    pub fqn: ContractFQN,
    /// Data for the contract (e.g., ABI)
    pub data: ContractData,
}

/// The address of a contract (blockchain independent)
pub type ContractAddress = Vec<u8>;

/// Information about a deployed contract
pub enum DeployedContract<M: Middleware> {
    /// An EVM-like contract
    Evm {
        /// The contract
        inner: EthersContract<M>,
    },
    /// A contract on Stellar
    Stellar {
        /// The address of the contract
        address: ContractAddress,
    },
}

/// Deployable contract.
pub struct Contract<M: Middleware = HttpStack> {
    /// Whether this is a shim.
    pub is_shim: bool,

    /// Project for the target chain (where this contract is deployed)
    pub project: Arc<TargetProject<M>>,

    /// Contract metadata
    pub meta: ContractInfo,

    /// Generated shims interfaces of this contract (one per other chain).
    ///
    /// Deploying a contract implies deploying all of its shims.
    /// Bridging is done only between the contracts within the same deployment cluster.
    ///
    /// If this contract is a shim itself, this map is empty.
    pub shims: HashMap<Target, Arc<Contract<M>>>,

    /// Shims of other contracts (on this target) that this contract is allowed to call.
    pub deps: Vec<Arc<Contract<M>>>,

    /// Ethers contract implementation, initialized once this contract is deployed (or loaded from a deployment receipt).
    inner: OnceCell<DeployedContract<M>>,
}

impl<M: Middleware> Contract<M> {
    /// Create new shim contract for a given chain project.
    ///
    /// # Arguments
    /// * `project` - [`TargetProject`] corresponding to the chain where this shim is to be deployed.
    /// * `meta`    - contract metadata (name, abi, bytecode, etc.)
    pub fn shim(project: Arc<TargetProject<M>>, meta: ContractInfo) -> Self {
        Contract {
            is_shim: true,
            project,
            meta,
            shims: HashMap::new(),
            deps: Vec::new(),
            inner: OnceCell::new(),
        }
    }

    /// Create new (non-shim) contract for a given chain project.
    ///
    /// # Arguments
    /// * `project` - [`TargetProject`] corresponding to the chain for which this contract is defined.
    /// * `shims`   - shims (interfaces) of this contract auto-generated for other chains.
    /// * `deps`    - shims (interfaces) of other contract (on this target) that this contract is allowed to call.
    /// * `meta`    - contract metadata (name, abi, bytecode, etc.)
    pub fn new(
        project: Arc<TargetProject<M>>,
        shims: HashMap<Target, Arc<Contract<M>>>,
        deps: Vec<Arc<Contract<M>>>,
        meta: ContractInfo,
    ) -> Self {
        assert!(
            shims.values().all(|s| s.is_shim),
            "'shims' must be shim contracts"
        );
        assert!(
            deps.iter().all(|s| s.is_shim),
            "'deps' must be shim contracts"
        );
        Contract {
            is_shim: false,
            project,
            meta,
            shims,
            deps,
            inner: OnceCell::new(),
        }
    }

    /// Returns a future that completes once this contract has been
    /// bridged (in which case the result is `true`) or when the
    /// timeout expires (in which case the result is `false`).
    ///
    /// # Arguments
    /// * `delays` - how long to wait between retries
    ///
    /// # Panics
    /// * if address is not initialized (either by calling `at` or `deploy`)
    pub async fn when_bridged(&self, delays: &Vec<Duration>) -> bool {
        assert!(self.is_deployed());

        if self.shims.is_empty() {
            return true;
        }

        if let BridgeInfo::Axelar(..) = self.project.bridge {
            return true;
        }

        let addr = self.address().or_bug("already checked that addr is set");
        let path = self
            .project
            .paths
            .notify_contract_bridged(&self.meta.fqn, &addr);
        trace!(
            "Waiting until {} is bridged (looking for file {})",
            self.name_with_target_and_address(),
            path.display()
        );
        for delay in delays {
            let is_file = tokio::fs::metadata(&path).await.map(|m| m.is_file());
            if let Ok(true) = is_file {
                debug!("{} is bridged", self.name_with_target_and_address());
                return true;
            }
            tokio::time::sleep(*delay).await;
        }
        let total_delay: Duration = delays.iter().sum();
        warn!(
            "{} not bridged after {total_delay:?}",
            self.name_with_target_and_address()
        );
        false
    }

    /// Full contract name in the format of {file_name}:{contract_name}
    pub fn full_name(&self) -> String {
        format!("{}:{}", self.meta.fqn.file.display(), self.meta.fqn.name)
    }

    /// Full contract name + the target chain.
    pub fn full_name_with_target(&self) -> String {
        format!("{}@{}", self.full_name(), self.target())
    }

    /// Contract name + deployed address + the target chain.
    pub fn name_with_target_and_address(&self) -> String {
        format!(
            "{}({})@{}",
            self.full_name(),
            hex(&self.address().unwrap_or_default()),
            self.target()
        )
    }

    /// Contract address @ target chain.
    pub fn address_and_target(&self) -> String {
        format!(
            "{}@{}",
            hex(&self.address().unwrap_or_default()),
            self.target()
        )
    }

    /// Address if the contract has been deployed (or loaded from disk), [`None`] otherwise.
    pub fn address(&self) -> Option<ContractAddress> {
        self.inner.get().map(|inner| match inner {
            DeployedContract::Evm { inner } => inner.address().as_fixed_bytes().to_vec(),
            DeployedContract::Stellar { address } => address.clone(),
        })
    }

    /// Address of deployed contract
    pub fn address_unsafe(&self) -> ContractAddress {
        self.address().unwrap()
    }

    /// The chain that this contract is targeting.
    pub fn target(&self) -> Target {
        self.project.target
    }

    /// Whether this contract is deployed.
    pub fn is_deployed(&self) -> bool {
        self.address().is_some()
    }

    /// Try to find deployed address of this contract or one of its
    /// shims corresponding to the given chain target.
    pub fn try_address_on(&self, target: Target) -> Option<ContractAddress> {
        if target == self.target() {
            self.address()
        } else {
            self.shims.get(&target).and_then(|c| c.address())
        }
    }

    /// Same as [`Self::try_address_on`] except that it panics instead
    /// of returning [`None`].
    ///
    /// # Panics
    ///
    /// If this contract has not been deployed on target `target`.
    pub fn address_on(&self, target: Target) -> ContractAddress {
        self.try_address_on(target).unwrap_or_else(|| {
            panic!(
                "Contract '{}' not deployed on target '{target}'",
                self.full_name()
            )
        })
    }

    /// First deploy all generated shims to their chains; then deploy this contract to its target chain.
    ///
    /// May be called multiple times; once deployed, subsequent calls become no-op.
    pub async fn deploy<T>(&self, args: T) -> Result<(ContractAddress, Arc<M>)>
    where
        T: Tokenize,
    {
        let span = span!(
            Level::DEBUG,
            "deploy",
            contract = self.meta.fqn.name,
            target = self.target().to_string()
        );
        let _enter = span.enter();

        let client = self.project.provider();

        // no-op if already deployed
        if let Some(address) = self.address() {
            trace!("Contract already deployed at {address:?}");
            return Ok((address, client));
        }

        // deploy shims if they're not already deployed
        self.deploy_shims().await?;

        // deploy self
        let address = self.deploy_self(args).await?;

        // If this contract is not a shim, grant it the CALLER role to
        // all its shim dependencies and save the deployment
        // manifest. (If it is a shim, it got/will get deployed via
        // (one of) its native contract(s) which will have the
        // manifest that involves the shim.)
        if !self.is_shim {
            self.update_shims(address.clone()).await?;
            self.make_approved_caller_for_shims().await?;
            self.save_deployment_manifest().await?;
        }

        Ok((address, client))
    }

    /// Deploy only this contract's shims.
    /// May be called multiple times; once deployed, subsequent calls become no-op
    /// (this is true because deploy_self has this property).
    pub async fn deploy_shims(&self) -> Result<()> {
        // nothing to do if no shims
        if self.shims.is_empty() {
            return Ok(());
        }

        match &self.project.bridge {
            BridgeInfo::Cubist => {
                // all shims have no-arg constructors
                for shim in self.shims.values() {
                    shim.deploy_self(()).await?;
                }
            }
            BridgeInfo::Axelar(m) => {
                // "axelar_receiver" shim (which is on the same chain) takes only gateway address
                let receiver_shim = self
                    .shims
                    .get(&self.target())
                    .or_bug("Same-target shim expected for Axelar bridge");
                let rec_addr = receiver_shim.deploy_self(m.gateway).await?;

                // "axelar_sender" shims (which are on different chains) take
                // (gateway, gas_receiver, axelar_receiver_shim_addr)
                for sender_shim in self.shims.values() {
                    if sender_shim.target() != self.target() {
                        if let BridgeInfo::Axelar(m) = &sender_shim.project.bridge {
                            sender_shim
                                .deploy_self((
                                    m.gateway,
                                    m.gas_receiver,
                                    hex(&rec_addr).to_string(),
                                ))
                                .await?;
                        } else {
                            panic!("[BUG] Expected: all target projects must use the same bridge provider; actual {} uses Axelar and {} doesn't", self.target(), sender_shim.target());
                        }
                    }
                }
            }
        };
        Ok(())
    }

    /// Deploy just self, ignore shims.
    /// May be called multiple times; once deployed, subsequent calls become no-op.
    async fn deploy_self<T>(&self, args: T) -> Result<ContractAddress>
    where
        T: Tokenize,
    {
        if self.inner.get().is_some() {
            return Ok(self.address_unsafe());
        }

        self.inner
            .get_or_try_init(|| {
                self.project
                    .deploy(&self.meta, args)
                    .map(|inner| inner.map(|inner| DeployedContract::Evm { inner }))
            })
            .await?;
        Ok(self.address_unsafe())
    }

    /// Set address of this contract.  Returns an error if address is already set and is different from `addr`.
    pub async fn at(&self, addr: &ContractAddress) -> Result<()> {
        if self.inner.get().is_some() {
            return match self.address_unsafe() == *addr {
                true => Ok(()),
                false => Err(CubistSdkError::DeployError(
                    self.meta.fqn.clone(),
                    self.target(),
                    Box::new(WrapperError::ContractError(
                        "Already initialized".to_owned(),
                    )),
                )),
            };
        }
        self.inner
            .get_or_try_init(|| self.create_inner(Address::from_slice(addr)))
            .await?;
        Ok(())
    }

    /// Set the address of a Soroban contract
    pub async fn set_soroban_addr(&self, addr: &ContractAddress) -> Result<()> {
        // TODO: Make nicer
        let _ = self.inner.set(DeployedContract::Stellar {
            address: addr.clone(),
        });
        Ok(())
    }

    /// Initialize this contract from its deployment receipt.  No-op
    /// if the contract is already deployed/initialized. Otherwise,
    /// succeeds only if there is exactly one corresponding deployment
    /// receipt found.  The return value is the contract address.
    pub async fn deployed(&self) -> Result<ContractAddress> {
        // load shims
        for shim in self.shims.values() {
            shim.deployed_self().await?;
        }

        // load self
        self.deployed_self().await
    }

    async fn deployed_self(&self) -> Result<ContractAddress> {
        if let Some(addr) = self.address() {
            return Ok(addr);
        }

        // TODO: Make nicer
        self.inner
            .get_or_try_init(|| {
                self.project
                    .deployed(&self.meta)
                    .map(|inner| inner.map(|inner| DeployedContract::Evm { inner }))
            })
            .await?;
        Ok(self
            .address()
            .expect("Must have address right after being loaded"))
    }

    async fn create_inner(&self, addr: Address) -> Result<DeployedContract<M>> {
        Ok(DeployedContract::Evm {
            inner: self.project.at(&self.meta, addr),
        })
    }

    /// Call a method on this contract.
    pub async fn call<TArgs, TRet>(&self, name: &str, args: TArgs) -> Result<TRet>
    where
        TArgs: Tokenize,
        TRet: Detokenize,
    {
        let call = self.method::<_, TRet>(name, args)?;
        let result = call
            .call()
            .await
            .map_err(|e| WrapperError::ContractError(e.to_string()))
            .map_err(|e| CubistSdkError::CallError {
                contract: self.meta.fqn.clone(),
                method_name: name.to_string(),
                target: self.project.target,
                source: Box::new(e),
            })?;
        Ok(result)
    }

    /// Send a transaction to this contract.
    pub async fn send<TArgs>(&self, name: &str, args: TArgs) -> Result<Option<TransactionReceipt>>
    where
        TArgs: Tokenize,
    {
        let span = span!(
            Level::DEBUG,
            "send",
            contract = self.meta.fqn.name,
            target = self.target().to_string(),
            method = name,
        );
        let _enter = span.enter();

        let call = self.method::<_, ()>(name, args)?;
        let receipt = self
            .project
            .send_tx(call.tx)
            .await
            .map_err(|e| self.to_call_error(name, e))?;
        Ok(Some(receipt))
    }

    /// Send a transaction to this contract.
    pub async fn send_soroban<TArgs>(&self, name: &str, args: TArgs) -> Result<()>
    where
        TArgs: Tokenize,
    {
        match self.inner()? {
            DeployedContract::Evm { .. } => todo!(),
            DeployedContract::Stellar { address } => {
                // TODO: Make identity configurable
                let identity = self.project.identities()?.first().unwrap().clone();

                Command::new("soroban")
                    .args(["contract", "invoke", "--id"])
                    .arg(String::from_utf8(address.clone()).unwrap())
                    .arg("--source")
                    .arg(identity)
                    .args(["--network", "standalone", "--", name])
                    .args(
                        &args
                            .into_tokens()
                            .into_iter()
                            .flat_map(|x| {
                                vec!["--num".to_string(), x.into_uint().unwrap().to_string()]
                            })
                            .collect::<Vec<_>>(),
                    )
                    .output()
                    .unwrap();
                Ok(())
            }
        }
    }

    /// Performs any shim updates after this (native) contract has been deployed
    async fn update_shims(&self, address: ContractAddress) -> Result<()> {
        match self.project.bridge {
            // no updates necessary
            BridgeInfo::Cubist => {}
            // only the "axelar_receiver" shim needs to be updated with the address of this contract
            BridgeInfo::Axelar(..) => {
                if let Some(rec_shim) = self.shims.get(&self.target()) {
                    debug!(
                        "Updating Axelar receiver shim's target by calling {} on {}",
                        backend::AXELAR_SET_TARGET_ADDR_METHOD_NAME,
                        rec_shim.name_with_target_and_address(),
                    );
                    rec_shim
                        .send(
                            backend::AXELAR_SET_TARGET_ADDR_METHOD_NAME,
                            Address::from_slice(&address),
                        )
                        .await?
                        .ok_or(CubistSdkError::AxelarSetTargetError {
                            receiver_contract: rec_shim.meta.fqn.clone(),
                            target: rec_shim.target(),
                        })?;
                }
            }
        };
        Ok(())
    }

    /// Grants CALLER role to this contract for all of its shim dependencies.
    async fn make_approved_caller_for_shims(&self) -> Result<()> {
        let address = self
            .address()
            .or_bug("Must be deployed before granting CALLER role");
        for dep in &self.deps {
            dep.deploy_self(()).await?;
            debug!(
                "Approving {} as a CALLER for shim contract {}",
                self.name_with_target_and_address(),
                dep.name_with_target_and_address()
            );
            dep.send(APPROVE_CALLER_METHOD_NAME, Address::from_slice(&address))
                .await?
                .ok_or(CubistSdkError::ApproveCallerError {
                    caller_contract: self.meta.fqn.clone(),
                    shim_contract: dep.meta.fqn.clone(),
                    target: self.target(),
                })?;
        }
        Ok(())
    }

    /// Bundle together deployed addresses of this contract and all of its shims.
    ///
    /// NOTE: that this overwrites any previous deployment manifest for this contract.
    /// TODO: consider changing this to instead fail
    pub async fn save_deployment_manifest(&self) -> Result<()> {
        let addr = self.address_unsafe();
        let to_info = |c: &Self| DeploymentInfo {
            target: c.target(),
            address: c.address_unsafe(),
        };
        let manifest = DeploymentManifest {
            contract: self.meta.fqn.clone(),
            deployment: to_info(self),
            shims: self.shims.values().map(Arc::as_ref).map(to_info).collect(),
        };
        let path = self
            .project
            .paths
            .for_deployment_manifest(&self.meta.fqn, &addr);

        manifest
            .write_atomic(&path)
            .map_err(|e| CubistSdkError::SaveDeploymentManifestError(path.clone(), Box::new(e)))?;

        debug!("Saved deployment manifest to {}", path.display());
        Ok(())
    }

    /// Return [`CubistSdkError`] to describe an error raised during calling a contract method.
    fn to_call_error(&self, method_name: &str, source: WrapperError) -> CubistSdkError {
        CubistSdkError::CallError {
            contract: self.meta.fqn.clone(),
            method_name: method_name.to_string(),
            target: self.project.target,
            source: Box::new(source),
        }
    }

    /// Find a contract method by its name and arguments.
    pub fn method<TArgs, TRet>(&self, name: &str, args: TArgs) -> Result<ContractCall<M, TRet>>
    where
        TArgs: Tokenize,
        TRet: Detokenize,
    {
        match self.inner()? {
            DeployedContract::Evm { inner } => inner
                .method::<_, TRet>(name, args)
                .map_err(WrapperError::ContractAbiError)
                .map_err(|e| CubistSdkError::CallError {
                    method_name: name.to_string(),
                    contract: self.meta.fqn.clone(),
                    target: self.project.target,
                    source: Box::new(e),
                }),
            DeployedContract::Stellar { .. } => todo!(),
        }
    }

    /// Return the inner (ethers) contract implementation if the contract has been deployed.
    pub fn inner(&self) -> Result<&DeployedContract<M>> {
        match self.inner.get() {
            Some(c) => Ok(c),
            None => Err(CubistSdkError::ContractNotDeployed {
                contract: self.meta.fqn.clone(),
                target: self.project.target,
            }),
        }
    }
}
