//! # CLI examples
//!
//! To start localnet chains for a given project, run:
//!
//! ```bash
//! cubist start chains
//! ```
//!
//! Then, to start a relayer for the same project, run:
//!
//! ```bash
//! cubist start relayer
//! ```
//!
//! Running Cubist services for two or more different projects in
//! parallel is ok, as long as the respective chains are configured
//! such that they don't clash with each other (e.g., use different
//! ports, different data directories, etc.).
//!
//! Trying to start a service for the project for which that service
//! is already running will result in a no-op.
//!
//! To see all running `cubist` services, execute
//!
//! ```bash
//! cubist status
//! ```
//!
//! which may output something like:
//!
//! ```text
//!   1. cubist:1860657 chains <proj1>/cubist-config.json
//!   2. cubist:1861437 chains <proj2>/cubist-config.json
//! ```
//!
//! You may also pass the `--json` flag to `status` in which
//! case the output will be more machine-consumable, e.g.,
//! ```json
//! [
//!   {
//!     "cubist_pid": 1860657,
//!     "info": {
//!       "kind": "Chains",
//!       "cubist_config": "<proj1>/cubist-config.json"
//!     }
//!   },
//!   {
//!     "cubist_pid": 1861437,
//!     "info": {
//!       "kind": "Chains",
//!       "cubist_config": "<proj2>/cubist-config.json"
//!     }
//!   }
//! ]
//!```
//!
//! To stop all running `cubist` services, execute
//!
//! ```bash
//! cubist stop
//! ```
//!
//! Both `status` and `stop` subcommands accept a filter that can be
//! used to narrow down the search, e.g.,
//!
//! ```bash
//! cubist stop --pid 1861437
//! cubist stop --config <proj1>/cubist-config.json
//! cubist stop --config <proj1>/cubist-config.json --kind relayer
//! ```
//!
//! # SDK Examples
//!
//! To achieve the same using the SDK, try something like:
//!
//! ```
//! use cubist_cli::daemon::{DaemonFilter, DaemonManager, StartArgs, StartMode, StartCommand};
//! use cubist_config::Config;
//! async {
//!     let cfg = Config::nearest().unwrap();
//!     let args = StartArgs::new(StartMode::Background, None);
//!     DaemonManager::start(cfg, args, StartCommand::Chains, false).await.unwrap();
//!     let list = DaemonManager::list(&Default::default());
//!     DaemonManager::stop(&Default::default()).unwrap();
//! };
//! ```
//!
//! # Implementation
//!
//! Metadata about running `cubist` processes is kept in
//! `$HOME/.cache/cubist_localchains/daemons` directory.  For example:
//!
//! ```text
//! [/home/aleks/.cache/cubist_localchains/daemons]
//! ├──[<proj1>]
//! │  ├── Chains-1860657.json
//! │  └── Chains-1860657.ready
//! └──[<proj2>]
//!    └── Chains-1861437.json
//! ```
//!
//! In this example, there are two running `cubist` processes
//! executing localnet chains for two different projects.
//! The chains for project `proj1` are ready to accept requests, while
//! for project `proj2` they are still bootstrapping.

use std::process::Stdio;
use std::{env, fmt::Display, path::PathBuf, time::Duration};

use crate::commands::relayer::RelayerConfig;
use crate::commands::{chain_manager, relayer};
use clap::{Args, Subcommand, ValueEnum};
use console::style;
use cubist_config::util::OrBug;
use cubist_config::Config;
use cubist_localchains::provider::WhileRunning;
use cubist_localchains::resource::DEFAULT_CACHE;
use cubist_util::proc::{kill_sync, SIGINT};
use eyre::{eyre, Result};
use scopeguard::defer;
use serde::{Deserialize, Serialize};
use tokio::signal::ctrl_c;
use tokio::{fs, process::Command};
use tracing::{debug, trace, warn};

#[derive(Debug, Args, Default)]
pub struct DaemonFilter {
    /// Limit to processes that were started using this config file.
    #[clap(short = 'c', long = "config", value_parser, value_hint = clap::ValueHint::FilePath)]
    pub config: Option<PathBuf>,
    /// Limit to the process with this process id.
    #[clap(short = 'p', long = "pid")]
    pub pid: Option<u32>,
    /// Limit to processes of this kind
    #[clap()]
    pub kind: Option<CubistServerKind>,
}

impl DaemonFilter {
    /// Maps [`DaemonFilter::config`] to its canonicalized version
    /// (path with all intermediate components normalized and symbolic
    /// links resolved).  If that path cannot be canonicalized,
    /// returns [`None`].
    pub fn config_canonicalized(&self) -> Option<PathBuf> {
        self.config
            .as_ref()
            .and_then(|p| std::fs::canonicalize(p).ok())
    }

    /// Returns a new instance with config path canonicalized.
    pub fn canonicalize(self) -> Self {
        DaemonFilter {
            config: self.config_canonicalized(),
            ..self
        }
    }
}

#[derive(Debug, Args, Default, Clone)]
pub struct StartArgs {
    /// How to start this cubist server process.
    #[clap(short = 'm', long = "mode")]
    pub mode: Vec<StartMode>,
    /// Location of the 'cubist' executable (`env.current_exe()` used
    /// by default)
    #[clap(skip)]
    pub cubist_exe: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum StartCommand {
    /// Start chains
    Chains,
    /// Start bridge
    Relayer(RelayerConfig),
}

impl StartCommand {
    pub fn kind(&self) -> CubistServerKind {
        match self {
            StartCommand::Chains => CubistServerKind::Chains,
            StartCommand::Relayer(_) => CubistServerKind::Relayer,
        }
    }

    // TODO: clap must be able to do this automatically
    pub fn to_args(&self) -> Vec<String> {
        match self {
            StartCommand::Chains => vec!["chains".into()],
            StartCommand::Relayer(args) => {
                let mut result = vec![
                    "relayer".into(),
                    format!("--watch-interval={}", args.watch_interval_ms),
                    format!("--max-events={}", args.max_events),
                ];
                if args.no_watch {
                    result.push("--no-watch".into());
                }
                result
            }
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum StartMode {
    /// Runs in the foreground only until all chains become ready,
    /// then exits leving the chains running in the background.
    Background,
    /// Runs in the foreground until interrupted; once interrupted, all
    /// chain processes are stopped.
    Foreground,
}

impl StartArgs {
    pub fn new(mode: StartMode, cubist_exe: Option<PathBuf>) -> Self {
        Self {
            mode: vec![mode],
            cubist_exe,
        }
    }

    pub fn daemonize(&self) -> bool {
        match self.mode.last() {
            Some(StartMode::Background) => true,
            Some(StartMode::Foreground) => false,
            None => true,
        }
    }

    pub fn cubist_exe(&self) -> PathBuf {
        match self.cubist_exe.as_ref() {
            Some(e) => e.clone(),
            None => env::current_exe().unwrap(),
        }
    }
}

/// Manifest file that describes a running cubist daemon process.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DaemonManifest {
    /// Process id of the `cubist` daemon.
    pub cubist_pid: u32,
    /// Additional info about the running daemon.
    pub info: DaemonInfo,
}

fn none_or_matches<T: Eq>(opt: Option<&T>, val: &T) -> bool {
    opt.is_none() || matches!(opt, Some(x) if *x == *val)
}

impl DaemonManifest {
    /// Returns whether this manifest matches a given filter.
    pub fn matches(&self, filter: &DaemonFilter) -> bool {
        none_or_matches(filter.pid.as_ref(), &self.cubist_pid)
            && none_or_matches(filter.config.as_ref(), &self.info.cubist_config)
            && none_or_matches(filter.kind.as_ref(), &self.info.kind)
    }
}

/// Various info about a running daemon.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DaemonInfo {
    /// What kind of operation this daemon is running
    pub kind: CubistServerKind,
    /// What's the config file that this daemon was configured to use
    pub cubist_config: PathBuf,
}

/// What kind of operation a cubist daemon is running
#[derive(Serialize, Deserialize, Debug, Clone, ValueEnum, Eq, PartialEq)]
pub enum CubistServerKind {
    /// Kind for a `cubist chains` daemon
    Chains,
    /// Kind for a `cubist relayer` daemon
    Relayer,
}

impl Display for CubistServerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Chains => "chains",
                Self::Relayer => "relayer",
            }
        )
    }
}

impl DaemonInfo {
    /// Full path to a folder where all daemon manifest files are saved
    fn global_cache_dir() -> PathBuf {
        DEFAULT_CACHE.join("daemons")
    }

    /// Full path to a folder where all daemon manifest files
    /// corresponding to a specific cubist project are written.
    fn project_cache_dir(&self) -> PathBuf {
        Self::global_cache_dir().join(base64::encode(
            self.cubist_config.to_str().or_bug("Cannot get config path"),
        ))
    }
}

impl DaemonManifest {
    /// Full path to a file that a `cubist chains` daemon writes
    /// to communicate that all of its servers are available.
    pub fn servers_ready_file(&self) -> PathBuf {
        self.path().with_extension("ready")
    }

    /// Writes an empty file to a specific location (to signal that
    /// all servers are available).
    pub async fn notify_service_ready(&self) -> Result<()> {
        let file = self.servers_ready_file();
        fs::create_dir_all(file.parent().or_bug("Must have a parent")).await?;
        fs::write(&file, "").await?;
        trace!("Saved 'notify ready' file to {}", file.display());
        Ok(())
    }

    /// Waits until all servers running under this daemon become
    /// available.
    ///
    /// Implementation: waits until [`Self::servers_ready_file`]
    /// appears on disk.
    pub async fn wait_service_ready(&self) {
        let path = self.servers_ready_file();
        trace!(
            "Waiting for servers to become ready (watching: {})",
            path.display()
        );
        while !path.is_file() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        trace!("All servers ready");
    }

    /// Full path to where this manifest is to be saved on disk.
    pub fn path(&self) -> PathBuf {
        self.info
            .project_cache_dir()
            .join(format!("{:?}-{}.json", self.info.kind, self.cubist_pid))
    }

    /// Saves this manifest to disk (to [`Self::path`])
    pub fn save(&self) -> Result<()> {
        let path = self.path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        if path.is_file() {
            warn!(
                "Overwriting existing daemon state for {self:?}: {}",
                &path.display()
            );
        }
        std::fs::write(&path, serde_json::to_string_pretty(&self)?)?;
        trace!("Saved '{self:?}' to {}", path.display());
        Ok(())
    }

    /// Deletes this daemon manifest from disk.
    pub fn delete(&self) {
        // remove the manifest file first
        let man_file = self.path();
        if man_file.is_file() {
            match std::fs::remove_file(&man_file) {
                Ok(_) => debug!("Deleted daemon manifest file {}", man_file.display()),
                Err(e) => debug!(
                    "Failed to delete daemon manifest file {}: {e}",
                    man_file.display()
                ),
            }
        }

        // remove the chains ready file
        let ready_file = self.servers_ready_file();
        if ready_file.is_file() {
            match std::fs::remove_file(&ready_file) {
                Ok(_) => debug!("Deleted servers ready file {}", ready_file.display()),
                Err(e) => debug!(
                    "Failed to delete servers ready file {}: {e}",
                    ready_file.display()
                ),
            }
        }
    }

    /// Stops this daemon process.
    pub fn stop(&self) -> Result<()> {
        kill_sync(self.cubist_pid, SIGINT)?;
        self.delete();
        Ok(())
    }
}

/// Namespace for managing running daemons
pub struct DaemonManager {}

impl DaemonManager {
    /// Starts a Cubist service, either in background or foreground.  If the
    /// former case, it exits once the service is up and running and
    /// ready to accept requests.  Otherwise, exits once interrupted.
    ///
    /// # Arguments
    /// * `cfg` - Cubist config
    /// * `args` - how to start service `cmd`
    /// * `cmd` - which service to start
    /// * `force_run_in_bg` - if set to `true` forces the service to run in the background (irrespective of `args`)
    pub async fn start(
        cfg: Config,
        args: StartArgs,
        cmd: StartCommand,
        force_run_in_bg: bool,
    ) -> Result<()> {
        let state = DaemonInfo {
            kind: cmd.kind(),
            cubist_config: cfg.config_path.to_path_buf(),
        };

        // check if already running
        let running = DaemonManager::list(&DaemonFilter {
            config: Some(state.cubist_config.clone()),
            pid: None,
            kind: Some(state.kind.clone()),
        });
        if !running.is_empty() {
            println!(
                "{}",
                style(format!(
                    "Already running cubist:{}.  Run 'cubist stop' first if you want to restart.",
                    running[0].cubist_pid
                ))
                .yellow()
            );
            return Ok(());
        }

        let daemonize = force_run_in_bg || args.daemonize();
        if daemonize {
            let mut child = Command::new(args.cubist_exe())
                .current_dir(env::current_dir()?)
                .arg("start")
                .arg("--config")
                .arg(&cfg.config_path)
                .arg("--mode=foreground")
                .args(cmd.to_args())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?;
            let child_pid = child
                .id()
                .ok_or(eyre!("Failed to get 'cubist' process ID"))?;
            let manifest = DaemonManifest {
                cubist_pid: child_pid,
                info: state,
            };
            child
                .while_running(manifest.wait_service_ready(), "cubist".to_string())
                .await?;
        } else {
            let manifest = DaemonManifest {
                cubist_pid: std::process::id(),
                info: state,
            };
            manifest.save()?;
            defer! { manifest.delete() }

            let servers = match cmd {
                StartCommand::Chains => chain_manager::start(cfg.network_profile()).await?,
                StartCommand::Relayer(args) => {
                    // synchronously wait until relayer is up and running, then let it run in a background thread
                    let relayer = relayer::start(cfg, args).await?;
                    tokio::spawn(async move {
                        relayer.run_to_completion().await.unwrap();
                    });
                    vec![]
                }
            };
            manifest.notify_service_ready().await?;
            ctrl_c().await?;
            drop(servers);
        }
        Ok(())
    }

    /// Returns a list of all running daemons that match a given filter.
    pub fn list(filter: &DaemonFilter) -> Vec<DaemonManifest> {
        let dir = DaemonInfo::global_cache_dir();
        let pattern = format!("{}/**/*.json", dir.display());
        let mut results = vec![];
        for file in glob::glob(&pattern)
            .or_bug("glob pattern should be valid")
            .flatten()
        {
            if let Ok(Ok(man)) = std::fs::read_to_string(file)
                .map(|content| serde_json::from_str::<DaemonManifest>(&content))
            {
                if man.matches(filter) {
                    results.push(man);
                }
            }
        }
        results
    }

    /// Prints out statuses of all running daemons that match a given filter.
    pub async fn status(filter: &DaemonFilter, json: bool) -> Result<usize> {
        if json {
            let statuses = Self::list(filter);
            println!("{}", serde_json::to_string_pretty(&statuses)?);
            return Ok(statuses.len());
        }

        let mut i = 0usize;
        for daemon in Self::list(filter) {
            i += 1;
            println!(
                "{i:3}. {} {} {}",
                style(format!("cubist:{}", daemon.cubist_pid))
                    .green()
                    .bold(),
                style(daemon.info.kind).magenta().dim(),
                style(daemon.info.cubist_config.display()).blue().dim()
            );
        }
        Ok(i)
    }

    /// Stops all running daemons that match a given filter.
    pub fn stop(filter: &DaemonFilter) -> Result<()> {
        for daemon in Self::list(filter) {
            println!(
                "{} {} {} {}",
                style("stopping").red().bold(),
                style(format!("cubist:{}", daemon.cubist_pid)).green().dim(),
                style(&daemon.info.kind).magenta().dim(),
                style(daemon.info.cubist_config.display()).blue().dim()
            );
            daemon.stop().unwrap_or_else(|e| {
                println!(
                    "{}",
                    style(format!("Failed to kill process {}: {e}", daemon.cubist_pid)).yellow()
                );
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_dm() -> DaemonManifest {
        DaemonManifest {
            cubist_pid: 123,
            info: DaemonInfo {
                kind: CubistServerKind::Chains,
                cubist_config: PathBuf::from("foo"),
            },
        }
    }

    fn assert_matches(expected: bool, dm: &DaemonManifest, f: &DaemonFilter) {
        assert_eq!(
            expected,
            dm.matches(f),
            "manifest = {dm:?}, filter = {f:?}, expected equal = {expected}"
        )
    }

    #[test]
    pub fn test_matches_empty_filter() {
        let dm = mk_dm();
        assert_matches(true, &dm, &Default::default());
        assert_matches(
            true,
            &dm,
            &DaemonFilter {
                config: None,
                pid: None,
                kind: None,
            },
        );
    }

    #[test]
    pub fn test_matches_config() {
        let dm = mk_dm();
        assert_matches(
            true,
            &dm,
            &DaemonFilter {
                config: Some(PathBuf::from("foo")),
                pid: None,
                kind: None,
            },
        );
        assert_matches(
            false,
            &dm,
            &DaemonFilter {
                config: Some(PathBuf::from("fo")),
                pid: None,
                kind: None,
            },
        );
    }

    #[test]
    pub fn test_matches_pid() {
        let dm = mk_dm();
        assert_matches(
            true,
            &dm,
            &DaemonFilter {
                config: None,
                pid: Some(123),
                kind: None,
            },
        );
        assert_matches(
            false,
            &dm,
            &DaemonFilter {
                config: None,
                pid: Some(1234),
                kind: None,
            },
        );
    }

    #[test]
    pub fn test_matches_kind() {
        let dm = mk_dm();
        assert_matches(
            true,
            &dm,
            &DaemonFilter {
                config: None,
                pid: None,
                kind: Some(CubistServerKind::Chains),
            },
        );
        assert_matches(
            false,
            &dm,
            &DaemonFilter {
                config: None,
                pid: None,
                kind: Some(CubistServerKind::Relayer),
            },
        );
    }

    #[test]
    pub fn test_matches_conj() {
        let dm = mk_dm();
        assert_matches(
            true,
            &dm,
            &DaemonFilter {
                config: Some(PathBuf::from("foo")),
                pid: Some(123),
                kind: Some(CubistServerKind::Chains),
            },
        );
        assert_matches(
            true,
            &dm,
            &DaemonFilter {
                config: None,
                pid: Some(123),
                kind: Some(CubistServerKind::Chains),
            },
        );
        assert_matches(
            false,
            &dm,
            &DaemonFilter {
                config: None,
                pid: Some(123),
                kind: Some(CubistServerKind::Relayer),
            },
        );
    }
}
