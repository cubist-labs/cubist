use cubist_cli::daemon::{
    DaemonFilter, DaemonManager, DaemonManifest, StartArgs, StartCommand, StartMode,
};
use cubist_config::Config;
use cubist_util::{
    proc::{kill, Signal},
    tasks::retry,
};
use eyre::{eyre, Result};
use serial_test::serial;
use std::{env, iter::repeat, path::PathBuf, process::Stdio, time::Duration};
use tokio::process::Command;

fn cfg(name: &str) -> Config {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("configs")
        .join(name);
    Config::from_file(path).unwrap()
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
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cubist"))
        .arg("start")
        .arg("--mode=foreground")
        .arg("--config")
        .arg(cfg.config_path)
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

#[tokio::test]
#[serial]
async fn test_start_status_stop() -> Result<()> {
    let cfg1 = cfg("cfg1.json");
    let cfg2 = cfg("cfg2.json");

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
    DaemonManager::stop(&DaemonFilter {
        config: Some(cfg1.config_path.clone()),
        pid: None,
        kind: None,
    })?;
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
