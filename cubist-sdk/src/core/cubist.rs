use std::{collections::HashMap, iter::repeat, sync::Arc, time::Duration};

use cubist_config::{paths::ContractFQN, util::OrBug, Config, Target};
use ethers::providers::Middleware;
use ethers::{prelude::Address, types::U256};
use futures::future::JoinAll;
use std::process::Command;

use crate::{core::ContractAddress, gen::backend::Backend, CubistSdkError, Result};

use super::{Contract, ContractInfo, HttpStack, TargetProject, TargetProjectInfo, WsStack};

type Map<K, V> = HashMap<K, V>;

/// Top-level type to use to manage configured contracts and target chains.
pub struct CubistInfo {
    /// Per-target projects.
    pub projects: Map<Target, TargetProjectInfo>,
    /// Per-target native (non-shim) contracts.
    pub contracts: Map<Target, Vec<ContractInfo>>,
    /// Per-target shim contracts.
    pub shims: Map<Target, Vec<ContractInfo>>,
    /// Underlying config.
    config: Config,
}

impl CubistInfo {
    /// Constructor
    pub fn new(config: Config) -> Result<Self> {
        let mut projects = Map::new();
        let mut contracts = Map::new();
        let mut shims = Map::new();

        for target in config.targets() {
            let proj = TargetProjectInfo::new(&config, target)?;
            projects.insert(target, proj);
        }

        for (target, proj) in &projects {
            let target_shims = proj.find_shim_contracts()?;
            shims.insert(*target, target_shims);

            let target_contracts = proj.find_contracts()?;
            contracts.insert(*target, target_contracts);
        }

        Ok(Self {
            projects,
            contracts,
            shims,
            config,
        })
    }

    /// Return all targets for which contract `fqn` has a shim.
    pub fn shim_targets<'a>(&'a self, fqn: &'a ContractFQN) -> impl Iterator<Item = Target> + '_ {
        self.shims.iter().filter_map(|(t, cs)| {
            if cs.iter().any(|c| c.fqn == *fqn) {
                Some(*t)
            } else {
                None
            }
        })
    }

    /// Return an iterator over all target-specific projects.
    pub fn projects(&self) -> impl Iterator<Item = &TargetProjectInfo> {
        self.projects.values()
    }

    /// Return project for a given target.
    pub fn project(&self, t: Target) -> Option<&TargetProjectInfo> {
        self.projects.get(&t)
    }

    /// Return an iterator over all non-shim contracts.
    pub fn contracts(&self) -> impl Iterator<Item = &ContractInfo> {
        self.contracts.values().flatten()
    }

    /// Return an iterator over all shim contracts.
    pub fn shims(&self) -> impl Iterator<Item = &ContractInfo> {
        self.shims.values().flatten()
    }

    /// Get the underlying config
    pub fn config(&self) -> &Config {
        &self.config
    }
}

/// Top-level type to use to manage configured contracts and target chains.
pub struct Cubist<M: Middleware = HttpStack> {
    /// Per-target projects.
    projects: Map<Target, Arc<TargetProject<M>>>,
    /// Per-target native (non-shim) contracts.
    contracts: Map<Target, Vec<Arc<Contract<M>>>>,
    /// Per-target shim contracts.
    shims: Map<Target, Vec<Arc<Contract<M>>>>,
    /// Underlying config.
    config: Config,
}

impl<M: Middleware> Clone for Cubist<M> {
    /// Create a new [`Cubist`] instance for the same Cubist project.
    /// [`Contract`]s returned by different [`Cubist`] instances are
    /// deployable independently of each other.
    fn clone(&self) -> Cubist<M> {
        Self::new_from_projects(self.projects.clone(), self.config.clone()).unwrap()
    }
}

macro_rules! cubist_constructor {
    ($config: expr, $ty: ty) => {{
        let mut projects = Map::new();
        for target in $config.targets() {
            let proj = TargetProjectInfo::new(&$config, target)?;
            let proj = TargetProject::<$ty>::create(proj).await?;
            projects.insert(target, Arc::new(proj));
        }
        Self::new_from_projects(projects, $config)
    }};
}

impl Cubist<HttpStack> {
    /// Create a new [`Cubist`] HTTP instance from a Cubist config.
    pub async fn new(config: Config) -> Result<Self> {
        cubist_constructor!(config, HttpStack)
    }
}

impl Cubist<WsStack> {
    /// Create a new [`Cubist`] WS instance from a Cubist config.
    pub async fn new(config: Config) -> Result<Self> {
        cubist_constructor!(config, WsStack)
    }
}

impl<M: Middleware> Cubist<M> {
    /// Create a new [`Cubist`] instance from a set of per-target-chain projects
    pub fn new_from_projects(
        projects: HashMap<Target, Arc<TargetProject<M>>>,
        config: Config,
    ) -> Result<Self> {
        let mut contracts = Map::new();
        let mut shims = Map::new();
        let backend = <dyn Backend>::create(&config);

        // pass 1: find shim contracts for each target
        for (target, proj) in &projects {
            let target_shims = proj
                .find_shim_contracts()?
                .into_iter()
                .map(|cc| Arc::new(Contract::shim(Arc::clone(proj), cc)))
                .collect::<Vec<_>>();
            shims.insert(*target, target_shims);
        }

        let find_shims = |t: &Target, cc: &ContractInfo| {
            shims
                .values()
                .flatten()
                .filter(|shim| backend.is_shim(t, &cc.fqn, &shim.target(), &shim.meta.fqn))
                .map(|shim| (shim.target(), Arc::clone(shim)))
                .collect::<HashMap<_, _>>()
        };

        // pass 2: create non-shim contracts for each target (each of which receives all of its shims)
        for (target, proj) in &projects {
            let target_shims = shims.get(target).or_bug("Shims for target missing");
            let target_contracts = proj
                .find_contracts()?
                .into_iter()
                .map(|cc| {
                    Arc::new(Contract::new(
                        Arc::clone(proj),
                        find_shims(target, &cc),
                        target_shims
                            .iter()
                            .filter(|shim| proj.is_dependency(&cc, &shim.meta))
                            .map(Clone::clone)
                            .collect::<Vec<_>>(),
                        cc,
                    ))
                })
                .collect::<Vec<_>>();
            contracts.insert(*target, target_contracts);
        }

        Ok(Cubist {
            projects,
            contracts,
            shims,
            config,
        })
    }

    /// Returns a future that completes once all initialized contracts in
    /// this project have been bridged (in which case the result is
    /// `true`) or when the timeout (of 10s by default) expires (in
    /// which case the result is `false`).
    ///
    /// # Arguments
    /// * `delays` - how long to wait between retries (defaults to 100ms for 100 times)
    pub async fn when_bridged(&self, delays: Option<Vec<Duration>>) -> bool {
        let delays =
            delays.unwrap_or_else(|| repeat(Duration::from_millis(100)).take(100).collect());
        self.contracts()
            .filter(|c| c.is_deployed())
            .collect::<Vec<_>>()
            .iter()
            .map(|c| c.when_bridged(&delays))
            .collect::<JoinAll<_>>()
            .await
            .iter()
            .all(|b| *b)
    }

    /// Find a (non-shim) contract by its name.
    pub fn contract(&self, name: &str) -> Option<Arc<Contract<M>>> {
        for tc in self.contracts.values() {
            for c in tc {
                if c.meta.fqn.name == name {
                    return Some(Arc::clone(c));
                }
            }
        }
        None
    }

    /// Find a contract for a target using its fully qualified name.
    pub fn find_contract(&self, target: Target, fqn: &ContractFQN) -> Option<Arc<Contract<M>>> {
        self.contracts
            .get(&target)
            .and_then(|contracts| contracts.iter().find(|c| c.meta.fqn == *fqn))
            .map(Arc::clone)
    }

    /// Find a shim for a target using its fully qualified name.
    pub fn find_shim(&self, target: Target, fqn: &ContractFQN) -> Option<Arc<Contract<M>>> {
        self.shims
            .get(&target)
            .and_then(|contracts| contracts.iter().find(|c| c.meta.fqn.is_same_as(fqn)))
            .map(Arc::clone)
    }

    /// Return an iterator over all target-specific projects.
    pub fn projects(&self) -> impl Iterator<Item = Arc<TargetProject<M>>> + '_ {
        self.projects.values().map(Arc::clone)
    }

    /// Return project for a given target.
    pub fn project(&self, t: Target) -> Option<Arc<TargetProject<M>>> {
        self.projects.get(&t).map(Arc::clone)
    }

    /// Return an iterator over all non-shim contracts.
    pub fn contracts(&self) -> impl Iterator<Item = Arc<Contract<M>>> + '_ {
        self.contracts.values().flatten().map(Arc::clone)
    }

    /// Get the underlying contract map.
    pub fn contract_map(&self) -> Map<Target, Vec<Arc<Contract<M>>>> {
        self.contracts.clone()
    }

    /// Return an iterator over all shim contracts.
    pub fn shims(&self) -> impl Iterator<Item = Arc<Contract<M>>> + '_ {
        self.shims.values().flatten().map(Arc::clone)
    }

    /// Get the underlying config
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Retrieve all managed accounts used on a target chain.
    pub async fn accounts_on(&self, t: Target) -> Result<Vec<Address>> {
        let proj = self.project(t).ok_or(CubistSdkError::ProjectError(t))?;
        proj.accounts().await
    }

    /// Get the balance of the given address on the target chain.
    pub async fn get_balance_on(&self, t: Target, a: Address) -> Result<U256> {
        let proj = self.project(t).ok_or(CubistSdkError::ProjectError(t))?;
        let balance = proj
            .provider()
            .get_balance(a, None)
            .await
            .map_err(|e| CubistSdkError::GetBalanceError(a.to_string(), t, e.to_string()))?;
        Ok(balance)
    }

    /// Deploy a Soroban contract
    pub fn deploy_soroban_contract(&self, contract: &Contract) -> Result<ContractAddress> {
        // TODO: Make identity configurable
        let identity = contract.project.identities()?.first().unwrap().clone();
        let endpoint = contract.project.endpoint_url()?.expose_url()?.to_string();

        // Ensure that identities are funded
        // TODO: fund at startup
        let identity_address = String::from_utf8(
            Command::new("soroban")
                .args(["config", "identity", "address"])
                .arg(identity.clone())
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        Command::new("curl")
            .arg(format!(
                "{endpoint}friendbot?addr={}",
                identity_address.trim()
            ))
            .output()
            .unwrap();

        let output = Command::new("soroban")
            .args(["contract", "deploy", "--source"])
            .arg(identity)
            .args(["--network", "standalone", "--wasm"])
            .arg(
                contract
                    .project
                    .paths
                    .for_target(Target::Stellar)
                    .contracts
                    .join(contract.meta.fqn.file.display().to_string()),
            )
            .output()
            .unwrap();
        if !output.status.success() {
            let error_msg = String::from_utf8(output.stderr).unwrap();
            Err(CubistSdkError::SorobanDeployError(
                contract.meta.fqn.clone(),
                error_msg,
            ))?;
        }
        Ok(String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .as_bytes()
            .to_vec())
    }
}
