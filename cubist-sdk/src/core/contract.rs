use crate::core::DeploymentInfo;
use crate::core::DeploymentManifest;
use crate::gen::APPROVE_CALLER_METHOD_NAME;
use crate::CubistSdkError;
use crate::Result;
use crate::WrapperError;
use cubist_config::paths::ContractFQN;
use cubist_config::util::OrBug;
use cubist_config::Target;
use ethers::abi::{Address, Detokenize, Tokenize};
use ethers::core::abi::Abi;
use ethers::prelude::builders::ContractCall;
use ethers::providers::Middleware;
use ethers::types::{Bytes, TransactionReceipt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;
use tracing::Level;
use tracing::{debug, span, trace, warn};

use super::{HttpStack, TargetProject};

type EthersContract<M> = ethers::contract::Contract<M>;

/// Contract compiled into abi and bytecode.
pub struct ContractInfo {
    /// Fully qualified name
    pub fqn: ContractFQN,
    /// Contract abi
    pub abi: Abi,
    /// Contract bytecode
    pub bytes: Bytes,
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
    inner: OnceCell<EthersContract<M>>,
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

        let addr = self.address().or_bug("already checked that addr is set");
        let path = self
            .project
            .paths
            .notify_contract_bridged(&self.meta.fqn, addr.as_fixed_bytes());
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
            self.meta.fqn.file.display(),
            self.address().unwrap_or_default(),
            self.target()
        )
    }

    /// Contract address @ target chain.
    pub fn address_and_target(&self) -> String {
        format!("{}@{}", self.address().unwrap_or_default(), self.target())
    }

    /// Address if the contract has been deployed (or loaded from disk), [`None`] otherwise.
    pub fn address(&self) -> Option<Address> {
        self.inner.get().map(|inner| inner.address())
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
    pub fn try_address_on(&self, target: Target) -> Option<Address> {
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
    pub fn address_on(&self, target: Target) -> Address {
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
    pub async fn deploy<T>(&self, args: T) -> Result<(Address, Arc<M>)>
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
            self.make_approved_caller_for_shims().await?;
            self.save_deployment_manifest().await?;
        }

        Ok((address, client))
    }

    /// Deploy only this contract's shims.
    /// May be called multiple times; once deployed, subsequent calls become no-op
    /// (this is true because deploy_self has this property).
    pub async fn deploy_shims(&self) -> Result<()> {
        for shim in self.shims.values() {
            shim.deploy_self(()).await?;
        }
        Ok(())
    }

    /// Deploy just self, ignore shims.
    /// May be called multiple times; once deployed, subsequent calls become no-op.
    async fn deploy_self<T>(&self, args: T) -> Result<Address>
    where
        T: Tokenize,
    {
        if let Some(inner) = self.inner.get() {
            return Ok(inner.address());
        }

        let inner = self
            .inner
            .get_or_try_init(|| self.project.deploy(&self.meta, args))
            .await?;
        Ok(inner.address())
    }

    /// Set address of this contract.  Returns an error if address is already set and is different from `addr`.
    pub async fn at(&self, addr: Address) -> Result<()> {
        if let Some(inner) = self.inner.get() {
            return match inner.address() == addr {
                true => Ok(()),
                false => Err(CubistSdkError::DeployError(
                    self.meta.fqn.clone(),
                    self.target(),
                    String::from("Already initialized"),
                )),
            };
        }
        self.inner
            .get_or_try_init(|| self.create_inner(addr))
            .await?;
        Ok(())
    }

    /// Initialize this contract from its deployment receipt.  No-op
    /// if the contract is already deployed/initialized. Otherwise,
    /// succeeds only if there is exactly one corresponding deployment
    /// receipt found.  The return value is the contract address.
    pub async fn deployed(&self) -> Result<Address> {
        // load shims
        for shim in self.shims.values() {
            shim.deployed_self().await?;
        }

        // load self
        self.deployed_self().await
    }

    async fn deployed_self(&self) -> Result<Address> {
        if let Some(addr) = self.address() {
            return Ok(addr);
        }

        self.inner
            .get_or_try_init(|| self.project.deployed(&self.meta))
            .await?;
        Ok(self
            .address()
            .expect("Must have address right after being loaded"))
    }

    async fn create_inner(&self, addr: Address) -> Result<EthersContract<M>> {
        Ok(self.project.at(&self.meta, addr))
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
                source: e,
            })?;
        Ok(result)
    }

    /// Send a transaction to this contract.
    pub async fn send<TArgs>(&self, name: &str, args: TArgs) -> Result<Option<TransactionReceipt>>
    where
        TArgs: Tokenize,
    {
        let call = self.method::<_, ()>(name, args)?;
        let receipt = call
            .send()
            .await
            .map_err(|e| WrapperError::ContractError(e.to_string()))
            .map_err(|e| self.to_call_error(name, e))?
            .await
            .map_err(|e| WrapperError::ProviderError("wait pending tx".to_owned(), e.to_string()))
            .map_err(|e| self.to_call_error(name, e))?;
        Ok(receipt)
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
            dep.send(APPROVE_CALLER_METHOD_NAME, address).await?.ok_or(
                CubistSdkError::ApproveCallerError {
                    caller_contract: self.meta.fqn.clone(),
                    shim_contract: dep.meta.fqn.clone(),
                    target: self.target(),
                },
            )?;
        }
        Ok(())
    }

    /// Bundle together deployed addresses of this contract and all of its shims.
    ///
    /// NOTE: that this overwrites any previous deployment manifest for this contract.
    /// TODO: consider changing this to instead fail
    async fn save_deployment_manifest(&self) -> Result<()> {
        let addr = self
            .address()
            .or_bug("Must be deployed before saving deployment manifest");
        let to_info = |c: &Self| DeploymentInfo {
            target: c.target(),
            address: c.address().or_bug("Must have address after deployment"),
        };
        let manifest = DeploymentManifest {
            contract: self.meta.fqn.clone(),
            deployment: to_info(self),
            shims: self.shims.values().map(Arc::as_ref).map(to_info).collect(),
        };
        let path = self
            .project
            .paths
            .for_deployment_manifest(&self.meta.fqn, addr.as_fixed_bytes());

        manifest
            .write_atomic(&path)
            .map_err(|e| CubistSdkError::SaveDeploymentManifestError(path.clone(), e))?;

        debug!("Saved deployment manifest to {}", path.display());
        Ok(())
    }

    /// Return [`CubistSdkError`] to describe an error raised during calling a contract method.
    fn to_call_error(&self, method_name: &str, source: WrapperError) -> CubistSdkError {
        CubistSdkError::CallError {
            contract: self.meta.fqn.clone(),
            method_name: method_name.to_string(),
            target: self.project.target,
            source,
        }
    }

    /// Find a contract method by its name and arguments.
    fn method<TArgs, TRet>(&self, name: &str, args: TArgs) -> Result<ContractCall<M, TRet>>
    where
        TArgs: Tokenize,
        TRet: Detokenize,
    {
        self.inner()?
            .method::<_, TRet>(name, args)
            .map_err(WrapperError::ContractAbiError)
            .map_err(|e| CubistSdkError::CallError {
                method_name: name.to_string(),
                contract: self.meta.fqn.clone(),
                target: self.project.target,
                source: e,
            })
    }

    /// Return the inner (ethers) contract implementation if the contract has been deployed.
    pub fn inner(&self) -> Result<&EthersContract<M>> {
        match self.inner.get() {
            Some(c) => Ok(c),
            None => Err(CubistSdkError::ContractNotDeployed {
                contract: self.meta.fqn.clone(),
                target: self.project.target,
            }),
        }
    }
}
