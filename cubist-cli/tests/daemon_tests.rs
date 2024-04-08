use cubist_cli::{
    commands::axelar,
    daemon::{DaemonFilter, DaemonManager, DaemonManifest, StartArgs, StartCommand, StartMode},
};
use cubist_config::{axelar_manifest::AxelarManifest, Config, Target};
use cubist_sdk::gen::backend::{AxelarBackend, AxelarNetwork};
use cubist_util::{
    proc::{kill, Signal},
    tasks::retry,
};
use eyre::{eyre, Result};
use scopeguard::defer;
use serial_test::serial;
use std::{env, iter::repeat, path::PathBuf, process::Stdio, time::Duration};
use tokio::process::Command;
use tokio::time::timeout;

fn cfg(name: &str) -> Config {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("configs")
        .join(name);
    let cfg = Config::from_file(path).unwrap();
    clean(&cfg).unwrap();
    cfg
}

fn cfg_stop(cfg: &Config) {
    DaemonManager::stop(&DaemonFilter {
        config: Some(cfg.config_path.clone()),
        pid: None,
        kind: None,
    })
    .unwrap();
}

async fn is_one_running() -> Result<DaemonManifest> {
    let running = DaemonManager::list(&Default::default());
    if running.is_empty() {
        Err(eyre!("None running"))
    } else if running.len() > 1 {
        Err(eyre!("More than one running"))
    } else {
        Ok(running[0].clone())
    }
}

async fn is_zero_running() -> Result<()> {
    let running = DaemonManager::list(&Default::default());
    if running.is_empty() {
        Ok(())
    } else {
        Err(eyre!("More than zero are running: {:?}", running))
    }
}

#[tokio::test]
#[serial]
async fn test_start_foreground() -> Result<()> {
    let delays = || repeat(Duration::from_millis(100)).take(100);

    // assert 'list' returns 0 entries to begin with
    println!("Checking initial state");
    retry(delays(), is_zero_running).await.unwrap();

    // start in foreground
    println!("Starting in foreground");
    let cfg = cfg("cfg1.json");
    defer! { cfg_stop(&cfg) }

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cubist"))
        .arg("start")
        .arg("--mode=foreground")
        .arg("--config")
        .arg(&cfg.config_path)
        .arg("chains")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    // assert 'list' returns 1 running
    println!("Querying running daemons");
    let dm = retry(delays(), is_one_running).await.unwrap();
    dm.wait_service_ready().await;

    // send SIGINT to the running one
    println!("Sending SIGINT");
    kill(dm.cubist_pid, Signal::Int).await.unwrap();

    // assert 'list' returns 0 entries
    println!("Querying again");
    retry(delays(), is_zero_running).await.unwrap();

    // assert the process terminates with success
    let status = cmd.wait().await?;
    assert!(status.success(), "Exit status: {status:?}");

    Ok(())
}

async fn start_chains(cfg: &Config, args: &StartArgs) -> Result<()> {
    DaemonManager::start(cfg.clone(), args.clone(), StartCommand::Chains, false).await
}

async fn start_axelar(cfg: &Config, args: &StartArgs) -> Result<()> {
    DaemonManager::start(cfg.clone(), args.clone(), StartCommand::Axelar, false).await
}

fn clean(cfg: &Config) -> Result<()> {
    for dir in [cfg.build_dir(), cfg.deploy_dir()] {
        if dir.is_dir() {
            std::fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_start_status_stop() -> Result<()> {
    let cfg1 = cfg("cfg1.json");
    let cfg2 = cfg("cfg2.json");
    defer! { cfg_stop(&cfg1); cfg_stop(&cfg2); }

    let args = StartArgs::new(
        StartMode::Background,
        Some(env!("CARGO_BIN_EXE_cubist").into()),
    );

    // start two instances
    println!("Starting 2 instances");
    start_chains(&cfg1, &args).await?;
    start_chains(&cfg2, &args).await?;

    let list = DaemonManager::list(&Default::default());
    assert_eq!(2, list.len());

    // stop one
    println!("Stopping one");
    cfg_stop(&cfg1);
    let list = DaemonManager::list(&Default::default());
    assert_eq!(1, list.len());
    assert_eq!(cfg2.config_path.clone(), list[0].info.cubist_config);

    // start the other one again while running (should be a no-op)
    println!("Starting the running one again (expecting no-op)");
    start_chains(&cfg2, &args).await?;
    let list = DaemonManager::list(&Default::default());
    assert_eq!(1, list.len());
    assert_eq!(cfg2.config_path.clone(), list[0].info.cubist_config);

    // start the first one back up
    println!("Starting the stopped one back up");
    start_chains(&cfg1, &args).await?;
    let list = DaemonManager::list(&Default::default());
    assert_eq!(2, list.len());

    // stop all
    println!("Stopping all");
    DaemonManager::stop(&Default::default())?;
    let list = DaemonManager::list(&Default::default());
    assert_eq!(0, list.len());

    Ok(())
}

fn assert_axelar_manifests(cfg: &Config) {
    let paths = cfg.paths();
    println!("Checking axelar manifests");
    for target in cfg.targets() {
        let file = &paths.for_target(target).axelar_manifest;
        println!("Checking {}", file.display());
        assert!(file.is_file(), "File {} not found", file.display());
        let contents = std::fs::read_to_string(file).unwrap();
        let man: AxelarManifest = serde_json::from_str(&contents).unwrap();
        let expected_chain_name =
            AxelarBackend::to_chain_name(target, AxelarNetwork::Localnet).to_string();
        assert_eq!(expected_chain_name, man.name);
        let chain_id = match target {
            Target::Ethereum => 31337,
            Target::Polygon => 1337,
            _ => unreachable!(),
        };
        assert_eq!(chain_id, man.chain_id);
    }
}

#[tokio::test]
#[serial]
async fn test_axelar() -> Result<()> {
    let cfg1 = cfg("cfg1.json");
    let cfg2 = cfg("cfg2.json");
    defer! { cfg_stop(&cfg1); cfg_stop(&cfg2); }

    let args = StartArgs::new(
        StartMode::Background,
        Some(env!("CARGO_BIN_EXE_cubist").into()),
    );

    // start chains for 2 projects
    println!("Starting chains for 2 projects");
    start_chains(&cfg1, &args).await?;
    start_chains(&cfg2, &args).await?;

    let list = DaemonManager::list(&Default::default());
    assert_eq!(2, list.len());

    // start axelar for the same 2 projects
    println!("Starting axelar for 2 projects");
    start_axelar(&cfg1, &args).await?;
    assert_axelar_manifests(&cfg1);
    start_axelar(&cfg2, &args).await?;
    assert_axelar_manifests(&cfg2);

    // stop one
    println!("Stopping one");
    cfg_stop(&cfg1);
    let list = DaemonManager::list(&Default::default());
    assert_eq!(2, list.len());
    assert_eq!(cfg2.config_path.clone(), list[0].info.cubist_config);
    assert_eq!(cfg2.config_path.clone(), list[1].info.cubist_config);

    // stop all
    println!("Stopping all");
    DaemonManager::stop(&Default::default())?;
    let list = DaemonManager::list(&Default::default());
    assert_eq!(0, list.len());

    // assert axelar manifest files are still there
    assert_axelar_manifests(&cfg1);
    assert_axelar_manifests(&cfg2);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_axelar_without_chains_fails() -> Result<()> {
    let cfg = cfg("cfg1.json");
    defer! { cfg_stop(&cfg) }

    let args = StartArgs::new(
        StartMode::Foreground,
        Some(env!("CARGO_BIN_EXE_cubist").into()),
    );

    // install dependencies separately so that that time doesn't count
    // toward the timeout below
    axelar::install_deps()?;

    println!("Starting axelar without chains");
    let result = timeout(Duration::from_secs(5), start_axelar(&cfg, &args)).await?;
    assert!(
        result.is_err(),
        "Expected error, got: {:?}",
        result.unwrap()
    );
    Ok(())
}
