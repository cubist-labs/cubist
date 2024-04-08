use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use clap::Args;
use console::style;
use cubist_sdk::{
    core::{Contract, Cubist, DeployedContract, DeploymentManifest},
    Http,
};
use ethers_contract::EthLogDecode;
use ethers_core::abi::{Address, Error, RawLog, Token};
use ethers_providers::Middleware;
use eyre::{bail, eyre, Result};

use cubist_config::{bridge::Bridge, paths::Paths, Config, EventName, FunctionName, Target};
use futures::{
    channel::mpsc::{self, Receiver, Sender},
    future::{select_all, try_join_all, JoinAll},
    SinkExt, StreamExt,
};
use tokio::{sync::Notify, task::JoinHandle};
use tracing::{debug, trace, warn};

use crate::{
    deployment_watcher::{DeploymentManifestWithPath, DeploymentWatcher},
    stylist,
};

/// Relayer configuration.
#[derive(Debug, Args)]
pub struct RelayerConfig {
    /// Disables watching the filesystem for new deployment receipts and automatically spinning up bridges for them.
    #[clap(short = 'w', long = "no-watch")]
    pub no_watch: bool,

    /// How often (in milliseconds) to poll the filesystem to check for new deployments.
    #[clap(
        short = 'd',
        long = "watch-interval",
        name = "MILLIS",
        default_value_t = RelayerConfig::default().watch_interval_ms
    )]
    pub watch_interval_ms: u64,

    /// Max number of events to process
    #[clap(short = 'e', long = "max-events", default_value_t = RelayerConfig::default().max_events)]
    pub max_events: u64,
}

impl RelayerConfig {
    /// Whether watching the deployment dir is enabled.
    pub fn watch(&self) -> bool {
        !self.no_watch
    }
}

impl Default for RelayerConfig {
    fn default() -> Self {
        Self {
            no_watch: false,
            watch_interval_ms: 500,
            max_events: std::u64::MAX,
        }
    }
}

impl RelayerConfig {
    fn watch_interval(&self) -> Duration {
        Duration::from_millis(self.watch_interval_ms)
    }
}

type BridgeTask = JoinHandle<Result<()>>;

struct DeploymentReceiver {
    watcher: DeploymentWatcher,
    rx: Receiver<DeploymentManifestWithPath>,
}

/// Spin up relaying for all shim contracts defined in this Cubist
/// project.
///
/// # Arguments
///
/// * `config` - Cubist configuration
/// * `args`   - relayer configuration
///
/// # Returns
///
/// A future that completes when the relayer is up and running, i.e.,
/// once it has created bridges for all existing deployments and
/// has started monitoring the deployment directory for new ones.
///
/// The result of the future is a [`Relayer`].  Don't forget to call
/// [`Relayer::run_to_completion`] on it (e.g., in a background
/// thread) to ensure received events are being processed/relayed.
pub async fn start(config: Config, args: RelayerConfig) -> Result<Relayer<Http>> {
    // kick off bridges for existing deployments (these tasks don't
    // complete unless a bridge fails or 'max_events' is reached)
    let cubist = Cubist::<Http>::new(config).await?;
    let mut relayer = Relayer::new(cubist, args)?;

    // start bridges for existing deployments
    relayer.start().await?;

    Ok(relayer)
}

/// Resolve all contracts listed in `manifest` against a given
/// [`Cubist`] instance.
///
/// # Returns
///
/// A vector containing one tuple for each shim contract listed in
/// `manifest`.  Each tuple has (1) the target contract, (2) the shim
/// contract, and (3) the bridge specifying how to connect the two
/// contracts.
async fn resolve_contracts<M: Middleware>(
    cubist: Cubist<M>,
    manifest: DeploymentManifest,
) -> Result<Vec<(Arc<Contract<M>>, Arc<Contract<M>>, Bridge)>> {
    let target = manifest.deployment.target;

    // find and initialize the main contract
    let target_contract = cubist
        .find_contract(target, &manifest.contract)
        .ok_or_else(|| {
            eyre!(
                "Contract '{}' not found for '{}'",
                &manifest.contract,
                target
            )
        })?;
    if target == Target::Stellar {
        target_contract
            .set_soroban_addr(&manifest.deployment.address)
            .await?;
    } else {
        target_contract.at(&manifest.deployment.address).await?;
    }

    // find and initialize all of its shims
    let mut result = Vec::new();
    for shim in &manifest.shims {
        let shim_contract = cubist
            .find_shim(shim.target, &manifest.contract)
            .ok_or_else(|| {
                eyre!(
                    "Shim '{}' not found for '{}'",
                    &manifest.contract,
                    &shim.target
                )
            })?;
        shim_contract
            .at(&Address::from_slice(&shim.address).as_fixed_bytes().to_vec())
            .await?;

        let bridge = shim_contract.project.load_bridge(&shim_contract.meta)?;
        result.push((shim_contract, Arc::clone(&target_contract), bridge));
    }

    Ok(result)
}

struct EvDe(RawLog);

impl EthLogDecode for EvDe {
    fn decode_log(log: &RawLog) -> Result<Self, Error>
    where
        Self: Sized,
    {
        Ok(EvDe(log.clone()))
    }
}

#[derive(Clone)]
struct SendRequest<M: Middleware> {
    fun_name: String,
    args: Vec<Token>,
    from: Arc<Contract<M>>,
    to: Arc<Contract<M>>,
}

/// Implements relaying
struct RelayerInner<M: Middleware> {
    /// Max number of events to process. (useful for testing)
    pub max_events: u64,
    /// Number of events processed so far.
    event_counter: Arc<AtomicU64>,
    /// Per-target transmission ends of bounded mpsc channels.
    senders: HashMap<Target, Sender<SendRequest<M>>>,
}

impl<M: Middleware> Clone for RelayerInner<M> {
    fn clone(&self) -> Self {
        Self {
            max_events: self.max_events,
            event_counter: self.event_counter.clone(),
            senders: self.senders.clone(),
        }
    }
}

/// Implements relaying of events from one chain to another.
pub struct Relayer<M: Middleware> {
    /// Relayer configuration
    args: RelayerConfig,
    /// The cubist instance
    cubist: Cubist<M>,
    /// Inner instance that may be cloned to create bridges on the fly.
    inner: RelayerInner<M>,
    /// Handles to all background tasks draining the receiver ends
    /// corresponding to `RelayerInner::senders`.
    drainers: Vec<JoinHandle<Result<()>>>,
    /// Channel for receiving deployments from [`DeploymentWatcher`].
    receiver: DeploymentReceiver,
    /// Tasks accumulated by calling `Relayer::start_bridge`.
    bridge_tasks: Vec<BridgeTask>,
}

impl<M: Middleware + 'static> Relayer<M> {
    /// Constructor.
    ///
    /// # Example
    ///
    /// ```
    /// use cubist_config::Config;
    /// use cubist_sdk::core::Cubist;
    /// use cubist_sdk::Ws;
    /// use cubist_cli::commands::relayer::{Relayer, RelayerConfig};
    ///
    /// async {
    ///   let config = Config::from_dir(".").unwrap();
    ///   let manifest_dir = config.paths().deployment_manifest_dir();
    ///   let cubist = Cubist::<Ws>::new(config).await.unwrap();
    ///   let args = RelayerConfig {
    ///       no_watch: true,
    ///       max_events: 10,
    ///       ..Default::default()
    ///   };
    ///   let mut relayer = Relayer::new(cubist, args).unwrap();
    ///
    ///   // returns once bridges for all existing deployments are installed
    ///   relayer.start().await.unwrap();
    ///
    ///   // returns once 'max_events' have been processed
    ///   relayer.run_to_completion().await.unwrap();
    /// };
    /// ```
    ///
    /// # Arguments
    /// * `cubist`- shared [`Cubist`] instance
    /// * `args`  - relayer configuration
    pub fn new(cubist: Cubist<M>, args: RelayerConfig) -> Result<Self> {
        let mut senders = HashMap::new();
        let mut receivers = Vec::new();
        for p in cubist.projects() {
            let (tx, rx) = mpsc::channel::<SendRequest<M>>(10);
            senders.insert(p.target, tx);
            receivers.push(rx);
        }

        let relayer = RelayerInner {
            max_events: args.max_events,
            event_counter: Arc::new(AtomicU64::new(0)),
            senders,
        };

        let drainers = receivers
            .into_iter()
            .map(Self::drain)
            .map(tokio::spawn)
            .collect::<Vec<_>>();

        let (tx, rx) = mpsc::channel(10);
        let watcher = DeploymentWatcher::new(tx, args.watch_interval())?;
        let receiver = DeploymentReceiver { watcher, rx };

        Ok(Relayer {
            args,
            cubist,
            inner: relayer,
            drainers,
            receiver,
            bridge_tasks: Vec::new(),
        })
    }

    /// Start bridges for existing deployments (found in a given
    /// `manifest_dir` directory).
    ///
    /// Also start watching `manifest_dir` if so configured (see [`RelayerConfig`]).
    ///
    /// # Returns
    ///
    /// A future that completes once all the bridges are spun up.
    pub async fn start(&mut self) -> Result<()> {
        let manifest_dir = self.cubist.config().paths().deployment_manifest_dir();

        // start bridges for existing deployments
        for dm in DeploymentWatcher::find_existing_deployments(&manifest_dir)
            .await?
            .into_iter()
            .filter(|dm| !dm.1.shims.is_empty())
        {
            self.bridges_for_deployment(self.cubist.clone(), dm).await?;
        }

        // start watching if so configured
        if self.args.watch() {
            println!(
                "{} {} dir",
                style("Watching").bold().blue(),
                &manifest_dir.display()
            );
            tokio::fs::create_dir_all(&manifest_dir).await?;
            self.receiver.watcher.watch(&manifest_dir)?;
        }

        Ok(())
    }

    /// Spin up bridges for everything described in a deployment manifest
    /// file but don't wait for any of them to complete.
    ///
    /// A deployment manifest contains deployed addresses of a **single**
    /// target contract and **all** of its shims).  Events are relayed
    /// from each shim to its target contract.  See
    /// [`Relayer::start_bridge`] for what it takes to bridge events
    /// between a shim and its target.
    ///
    /// # Arguments
    ///
    /// * `cubist`   - an exclusive [`Cubist`] instance
    /// * `manifest` - metadata about contract deployments
    ///
    /// # Returns
    ///
    /// A future that completes when all necessary bridges are spun up
    /// (i.e., when they have already regestered listeners for all
    /// events of interest).
    async fn bridges_for_deployment(
        &mut self,
        cubist: Cubist<M>,
        manifest: DeploymentManifestWithPath,
    ) -> Result<()> {
        for (from, to, bridge) in resolve_contracts(cubist, manifest.1).await? {
            self.start_bridge(from, to, bridge).await?;
        }
        let new_name = Paths::bridged_signal_for_manifest_file(&manifest.0);
        trace!("Renamed deployment manifest {}", new_name.display());
        tokio::fs::rename(&manifest.0, new_name).await?;
        Ok(())
    }

    /// Start bridging events between a pair of deployed contracts and
    /// return immediately after.
    ///
    /// Created bridges are remembered and awaited when
    /// [`Self::run_to_completion`] is called.
    ///
    /// # Returns
    ///
    /// A future that completes once subscribed and already listening
    /// for all events of interest.
    pub async fn start_bridge(
        &mut self,
        from: Arc<Contract<M>>,
        to: Arc<Contract<M>>,
        bridge: Bridge,
    ) -> Result<()> {
        let mut tasks = self.inner.start_bridge(from, to, bridge).await?;
        self.bridge_tasks.append(&mut tasks);
        Ok(())
    }

    /// If watching is enabled, it continues to watch the deployment
    /// dir forever; otherwise, returns once
    /// [`RelayerConfig::max_events`] number of events has been
    /// relayed.
    pub async fn run_to_completion(mut self) -> Result<()> {
        // Receive all events from the watcher (if configured to watch)
        if self.args.watch() {
            while let Some(dm) = self.receiver.rx.next().await {
                let contract_name = dm.1.contract.to_string();
                self.bridges_for_deployment(self.cubist.clone(), dm)
                    .await
                    .unwrap_or_else(|e| {
                        let msg = format!("Failed to start a bridge for {contract_name}: {e}");
                        println!("{}", style(msg).yellow().bold());
                    });
            }
        }

        // Why one: when `max_events` is reached, only one task will complete.
        if !self.bridge_tasks.is_empty() {
            let result = select_all(self.bridge_tasks).await;
            result.0??;
            // drop everything else (to ensure that the transmission
            // ends of the mpsc channels are closed)
            result.2.into_iter().for_each(|h| {
                h.abort();
                drop(h);
            });
        }

        // Wait for all drainer futures to complete pending updates
        drop(self.inner);
        trace!("Waiting for all drainers to finish");
        try_join_all(self.drainers)
            .await?
            .into_iter()
            .collect::<Result<_>>()?;

        Ok(())
    }

    /// Keeps reading from a given receiving end of a bounded buffer
    /// until it is fully drained. Relays each item read from the
    /// buffer to its target chain.
    async fn drain(mut rx: Receiver<SendRequest<M>>) -> Result<()> {
        while let Some(req) = rx.next().await {
            let args_str = req
                .args
                .iter()
                .map(|a| format!("{a}"))
                .collect::<Vec<_>>()
                .join(", ");

            let fun_with_args = &format!("{}({args_str})", &req.fun_name);
            let trace_prefix = trace_prefix(fun_with_args, &req.from, &req.to);
            println!(" {} {trace_prefix}", style("sending").green().dim());

            if req.to.target() == Target::Stellar {
                req.to
                    .send_soroban(&req.fun_name, Token::Tuple(req.args.clone()))
                    .await?;
            } else {
                let receipt = req
                    .to
                    .send(&req.fun_name, Token::Tuple(req.args.clone()))
                    .await?;

                println!("    {} {trace_prefix}", style("SENT").green().bold());
                match receipt {
                    Some(r) => trace!("[{trace_prefix}] Transaction receipt: {r:?}"),
                    None => warn!("[{trace_prefix}] Receipt is empty"),
                }
            }
        }

        Ok(())
    }
}

impl<M: Middleware + 'static> RelayerInner<M> {
    /// Start bridging events between a pair of deployed contracts and
    /// return immediately after.
    ///
    /// Concretely, starts a number of background tasks, one for each
    /// event listed in `bridge` to be relayed between the two contracts.
    ///
    /// # Arguments
    ///
    /// * `from`   - shim contract whose events to relay to a target contract
    /// * `to`     - receiver contract, to which to relay events from `from`
    /// * `bridge` - bridge metadata that specifies which events from the source
    ///              contract to relay to which functions of the target contract.
    ///
    /// # Returns
    ///
    /// A future that completes once subscribed and already listening
    /// for all events of interest.
    ///
    /// The result of that future is a vector of join handles to all
    /// spawned bridge tasks.
    ///
    /// # Panics
    ///
    /// * if either contract has not been deployed
    /// * if `from` is not a shim
    /// * if `to` is a shim
    /// * if either contract doesn't match the data in `bridge`
    pub async fn start_bridge(
        &self,
        from: Arc<Contract<M>>,
        to: Arc<Contract<M>>,
        bridge: Bridge,
    ) -> Result<Vec<BridgeTask>> {
        // precondition checks
        assert_eq!(
            bridge.receiver_target(),
            to.target(),
            "The target of the receiver contract ({}) does not match the target of the bridge ({})",
            to.target(),
            bridge.receiver_target()
        );
        assert!(
            from.is_shim,
            "Cannot start a bridge from contract '{}' because it is not a shim",
            from.meta.fqn
        );
        assert!(
            !to.is_shim,
            "Cannot start a bridge to contract '{}' because it is a shim",
            to.meta.fqn
        );
        for c in [&from, &to] {
            assert!(
                c.is_deployed(),
                "Contract '{}' must first be deployed before a bridge can be started for it",
                c.meta.fqn
            );
        }

        debug!(
            "Starting bridges between {} and {}",
            &from.full_name_with_target(),
            &to.full_name_with_target()
        );

        // start an async task for each event from this shim that needs bridging
        let tasks = bridge
            .bridges(&from.meta.fqn.name)
            .map(|(fun_name, ev_name)| {
                let notify_ready = Arc::new(Notify::new());
                let bridge_future = self.clone().relay_events(
                    Arc::clone(&notify_ready),
                    Arc::clone(&from),
                    Arc::clone(&to),
                    fun_name.clone(),
                    ev_name.clone(),
                );
                (notify_ready, tokio::spawn(bridge_future))
            })
            .collect::<Vec<_>>();

        // wait for all to subscribe to events and start streaming
        tasks
            .iter()
            .map(|t| t.0.notified())
            .collect::<JoinAll<_>>()
            .await;

        // return running tasks
        let result: Vec<_> = tasks.into_iter().map(|t| t.1).collect();
        debug!(
            "Started bridging {} event(s) between {} and {}",
            result.len(),
            &from.full_name_with_target(),
            &to.full_name_with_target()
        );
        Ok(result)
    }

    /// Indefinitely keep relaying events from one contract (`from`)
    /// by calling a function of another contract (`to`), or until
    /// `max_events` count is reached..
    ///
    /// # Arguments
    ///
    /// * `notify_ready` - a handle to notify once subscribed and already listening for events
    /// * `from`         - the contract whose events to subscribe to
    /// * `to`           - the contract to which to forward the received events
    /// * `fun_name`     - the function to call on the receiver contract when forwarding an event
    /// * `ev_name`      - the event of contract `from` to subscribe to and relay to contract `to`
    ///
    /// # Returns
    ///
    /// A future that completes once the count of processed events
    /// reaches `max_events`.
    ///
    /// # Panics
    ///
    /// * if either `from` or `to` is not deployed
    /// * if `from` is not a shim contract
    /// * if `to` is a shim contract
    async fn relay_events(
        mut self,
        notify_ready: Arc<Notify>,
        from: Arc<Contract<M>>,
        to: Arc<Contract<M>>,
        fun_name: FunctionName,
        ev_name: EventName,
    ) -> Result<()> {
        debug_assert!(from.is_deployed());
        debug_assert!(from.is_shim);
        debug_assert!(to.is_deployed());
        debug_assert!(!to.is_shim);

        let ethers_contract = from.inner()?;
        let ev = match ethers_contract {
            DeployedContract::Evm { inner } => inner.event_for_name::<EvDe>(&ev_name)?,
            DeployedContract::Stellar { .. } => todo!(),
        };

        // notify that streaming has started then stream until max count is reached
        let trace_prefix = trace_prefix(&fun_name, &from, &to);
        println!("{} {trace_prefix}", style("Bridging").bold().green());
        notify_ready.notify_one();
        let mut stream = ev.stream().await?;

        while self.event_counter.load(Ordering::Relaxed) < self.max_events {
            debug!("[{trace_prefix}] Listening for events",);
            let log: RawLog = match stream.next().await {
                Some(Ok(res)) => res.0,
                Some(Err(e)) => {
                    warn!("[{trace_prefix}] Failed to decode an event: {e}.");
                    bail!("{e}");
                }
                None => break,
            };

            self.forward_event(log, &from, &to, &fun_name, &ev_name)
                .await?;
            let old_cnt = self.event_counter.fetch_add(1, Ordering::Relaxed);
            trace!("[{trace_prefix}] Scheduled another event.  Total number of events processed so far: {}", old_cnt + 1);
        }

        debug!("[{trace_prefix}] Done bridging");
        Ok(())
    }

    /// Schedule a single event received from contract `from` to be
    /// forwarded to target contract `to` (by calling its `fun_name`
    /// function). The target contract is updated from a separate
    /// processing task, to ensure absence of errors caused by
    /// concurrent updates.
    ///
    /// # Arguments
    ///
    /// * `log`      - encoded event arguments to be passed when calling the target function
    /// * `from`     - parent contract of the received event
    /// * `to`       - the contract to which to forward the received event
    /// * `fun_name` - the name of the function to call on contract `to`
    /// * `ev_name`  - the name of the received event
    ///
    /// # Returns
    ///
    /// A future that completes once the event has been scheduled for
    /// forwarding (but not necessarily yet executed on the target
    /// contract).
    async fn forward_event(
        &mut self,
        log: RawLog,
        from: &Arc<Contract<M>>,
        to: &Arc<Contract<M>>,
        fun_name: &FunctionName,
        ev_name: &EventName,
    ) -> Result<()> {
        let args = match from.inner()? {
            DeployedContract::Evm { inner } => {
                inner.decode_event_raw(ev_name, log.topics, log.data.into())?
            }
            DeployedContract::Stellar { .. } => todo!(),
        };

        let trace_prefix = trace_prefix(fun_name, from, to);
        trace!("[{trace_prefix}] Received: {args:?}; scheduling for execution on target chain");

        let tx = self.senders.get_mut(&to.target()).unwrap();
        tx.send(SendRequest {
            fun_name: fun_name.clone(),
            args,
            from: Arc::clone(from),
            to: Arc::clone(to),
        })
        .await?;
        Ok(())
    }
}

fn trace_prefix<M: Middleware>(
    fun_name: &str,
    from: &Arc<Contract<M>>,
    to: &Arc<Contract<M>>,
) -> String {
    let what = stylist::event(format!("{}::{fun_name}", from.meta.fqn.name));
    format!(
        "{what} ({} -> {})",
        stylist::sender(from.address_and_target()),
        stylist::receiver(to.address_and_target()),
    )
}
