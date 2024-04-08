use assert_matches::assert_matches;
use color_eyre::owo_colors::OwoColorize;
use cubist_cli::{
    commands::relayer::{Relayer, RelayerConfig},
    cube::template::Template,
    daemon::{CubistServerKind, DaemonFilter, DaemonManager, StartArgs, StartCommand, StartMode},
};
use cubist_config::{paths, util::OrBug, Config, ProjType, Target};
use cubist_sdk::{
    core::{
        Contract, Cubist, DeploymentInfo, DeploymentManifest, TargetProject, TargetProjectInfo,
    },
    gen::APPROVE_CALLER_METHOD_NAME,
    CubistSdkError, Http, Ws,
};
use cubist_util::proc::SIGINT;
use ethers_core::{abi::Address, types::U256};
use ethers_providers::Middleware;
use eyre::{Context, Result};
use rstest::rstest;
use scopeguard::defer;
use serde_json::json;
use serial_test::serial;
use std::{
    env, fs,
    iter::repeat,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tempfile::tempdir;
use tokio::process::Command;
use tokio::try_join;
use toml::{toml, Value::Table};

enum ApiKind {
    Low,
    Mid,
    Orm,
}

type EthersContract<M> = ethers_contract::Contract<M>;

const TEST_BRANCH: &str = "main";

fn create_relayer<M: Middleware + 'static>(proj: &Cubist<M>, max_events: u64) -> Relayer<M> {
    Relayer::new(
        proj.clone(),
        RelayerConfig {
            max_events,
            no_watch: true,
            ..Default::default()
        },
    )
    .unwrap()
}

async fn start_services(cfg: &Config) -> Result<()> {
    start_chains(cfg.clone()).await?;
    start_relayer(cfg.clone()).await
}

async fn start_chains(cfg: Config) -> Result<()> {
    start_cmd(cfg, StartCommand::Chains).await
}

async fn start_relayer(cfg: Config) -> Result<()> {
    start_cmd(cfg, StartCommand::Relayer(Default::default())).await
}

async fn start_cmd(cfg: Config, cmd: StartCommand) -> Result<()> {
    let args = StartArgs::new(
        StartMode::Background,
        Some(env!("CARGO_BIN_EXE_cubist").into()),
    );
    let force_run_in_bg = true;
    DaemonManager::start(cfg, args, cmd, force_run_in_bg).await
}

fn stop_services(cfg_path: PathBuf) {
    stop(cfg_path, None)
}

fn stop_chains(cfg_path: PathBuf) {
    stop(cfg_path, Some(CubistServerKind::Chains))
}

fn stop(cfg_path: PathBuf, kind: Option<CubistServerKind>) {
    let filter = DaemonFilter {
        config: Some(cfg_path),
        kind,
        ..Default::default()
    };
    DaemonManager::stop(&filter).unwrap()
}

fn clean(app_dir: &Path) {
    let rmdir = |dir: &PathBuf| {
        if dir.is_dir() {
            fs::remove_dir_all(dir).unwrap();
        }
    };

    rmdir(&app_dir.join("build"));
    rmdir(&app_dir.join("deploy"));
    rmdir(&app_dir.join("node_modules"));
}

fn project_fixtures_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("projects")
}

fn project_fixture_dir(name: &str) -> PathBuf {
    project_fixtures_dir().join(name)
}

/// Updates `{app_dir}/Cargo.toml` to use local `cubist-sdk` and
/// `cubist-config` crates instead of pulling from github@main. This
/// ensures we are testing against the current code and not against
/// what's in main
fn update_cargo_toml(app_dir: &Path) -> Result<()> {
    let man_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cubist_root = man_dir.parent().unwrap();
    let cubist_config_path = cubist_root.join("cubist-config").display().to_string();
    let cubist_sdk_path = cubist_root.join("cubist-sdk").display().to_string();
    let cubist_target_dir = cubist_root.join("target").display().to_string();

    let toml_file = app_dir.join("Cargo.toml");
    let toml_contents = std::fs::read_to_string(&toml_file)?;
    let mut tml: toml::Value = toml::from_str(&toml_contents)?;

    // update cubist-sdk dependency
    let dependencies = tml.get_mut("dependencies").unwrap().as_table_mut().unwrap();
    dependencies.insert(
        String::from("cubist-sdk"),
        Table(toml! { path = cubist_sdk_path }),
    );
    dependencies.insert(
        String::from("cubist-config"),
        Table(toml! { path = cubist_config_path }),
    );

    let toml_contents = toml::to_string_pretty(&tml)?;
    std::fs::write(&toml_file, toml_contents)?;

    // also write .cargo/config.toml file to app_dir and set
    // 'build.target-dir' to match the target dir or the Cubist
    // workspace
    let cargo_config_file = app_dir.join(".cargo").join("config.toml");
    std::fs::create_dir_all(cargo_config_file.parent().unwrap())?;
    std::fs::write(
        &cargo_config_file,
        toml::to_string_pretty(&toml! {
            build.target-dir = cubist_target_dir
        })?,
    )?;

    // Copy Cargo.lock to maximize the reuse of libraries that have already been compiled
    std::fs::copy(cubist_root.join("Cargo.lock"), app_dir.join("Cargo.lock"))?;

    Ok(())
}

async fn deploy_and_test_orm_api(cfg: &Config) -> Result<()> {
    // run 'relayer' to start bridging events
    let mut relayer_cmd = cubist_cmd()
        .arg("start")
        .arg("--mode=foreground")
        .arg("relayer")
        .current_dir(cfg.project_dir())
        .spawn()?;

    // update generated Cargo.toml to use local cubist-sdk and cubist-config crates
    update_cargo_toml(&cfg.project_dir())?;

    // run deployment script
    let cargo_run_status = Command::new("cargo")
        .arg("run")
        .current_dir(&cfg.project_dir())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    cubist_util::proc::kill(relayer_cmd.id().unwrap(), SIGINT).await?;
    relayer_cmd.wait().await?;
    let filter = DaemonFilter {
        kind: Some(CubistServerKind::Relayer),
        config: Some(cfg.config_path.clone()),
        ..Default::default()
    };
    assert_eq!(0, DaemonManager::list(&filter).len());
    assert!(cargo_run_status.success());
    Ok(())
}

async fn deploy_and_test_api_mid(cfg: Config) -> Result<()> {
    let proj = Cubist::<Http>::new(cfg.clone()).await?;
    let receiver = proj.contract("StorageReceiver").unwrap();
    let sender = proj.contract("StorageSender").unwrap();

    // we haven't deployed the contracts so trying to load contract from existing receipt should fail
    assert_matches!(
        receiver.deployed().await,
        Err(CubistSdkError::LoadContractNoReceipts(..))
    );
    assert_matches!(
        sender.deployed().await,
        Err(CubistSdkError::LoadContractNoReceipts(..))
    );

    // deploy StorageReceiver and assert it deployed on both chains
    println!("Deploying");
    let val = U256::from(42);
    receiver.deploy(val).await?;
    assert_deployment_manifest(&receiver)?;

    // deploy StorageSender
    sender
        .deploy((
            val,
            Address::from_slice(&receiver.address_on(sender.target())),
        ))
        .await?;
    assert_deployment_manifest(&sender)?;

    // start the bridge right away (and tell it to process up to 3 events, so that we can wait for it to finish)
    let mut relayer = create_relayer(&proj, 3);
    relayer.start().await?;

    // call retrieve to test constructors
    println!("Testing constructor");
    assert_eq!(val, receiver.call("retrieve", ()).await?);
    assert_eq!(val, sender.call("retrieve", ()).await?);

    // call 'store' on polygon multiple times and assert that all of them will be bridged over to ethereum
    println!("Testing 'store'");
    let val = U256::from(52);
    sender.send("store", val).await?;
    let val = U256::from(62);
    sender.send("store", val).await?;
    let val = U256::from(72);
    sender.send("store", val).await?;

    assert_eq!(val, sender.call("retrieve", ()).await?);

    // wait for the bridge to process 3 events
    relayer.run_to_completion().await?;
    assert_eq!(val, receiver.call("retrieve", ()).await?);

    // Create a new Cubist instance and try to load contracts from existing receipts
    let cube = Cubist::<Http>::new(cfg.clone()).await?;
    let receiver_loaded = cube.contract("StorageReceiver").unwrap();
    receiver_loaded.deployed().await?;
    receiver_loaded.deployed().await?; // loading multiple times should work
    let sender_loaded = cube.contract("StorageSender").unwrap();
    sender_loaded.deployed().await?;
    sender_loaded.deployed().await?; // loading multiple times should work
    assert_eq!(receiver.address(), receiver_loaded.address());
    assert_eq!(
        receiver.address_on(sender.target()),
        receiver_loaded.address_on(sender.target())
    );
    assert_eq!(sender.address(), sender_loaded.address());

    Ok(())
}

fn assert_receipt(proj: &TargetProject, file: &str, name: &str, addr: &[u8]) {
    let path = proj
        .target_paths
        .deploy_root
        .join(file)
        .join(name)
        .join(paths::hex(addr))
        .with_extension("json");
    assert!(path.is_file(), "Receipt not found at {}", path.display());
}

fn assert_deployment_manifest(cnt: &Contract) -> Result<()> {
    // check deployment manifest
    let manifest_path = cnt
        .project
        .paths
        .for_deployment_manifest(&cnt.meta.fqn, &cnt.address().unwrap());
    assert!(
        manifest_path.is_file(),
        "Deployment manifest not found at {}",
        manifest_path.display()
    );
    let contents = std::fs::read_to_string(manifest_path)?;
    let manifest: DeploymentManifest = serde_json::from_str(contents.as_str())?;

    let check_deployment_info = |c: &Contract, d: &DeploymentInfo| {
        assert_eq!(c.address(), Some(d.address.clone()));
        assert_eq!(c.target(), d.target);
    };

    // check info for self
    assert_eq!(cnt.meta.fqn, manifest.contract);
    check_deployment_info(cnt, &manifest.deployment);

    // check infos for shims
    assert_eq!(cnt.shims.len(), manifest.shims.len());
    for shim in cnt.shims.values() {
        let deployment_info = manifest
            .shims
            .iter()
            .find(|c| c.target == shim.target())
            .unwrap_or_else(|| {
                panic!(
                    "No deployment info found for shim '{}'",
                    shim.full_name_with_target()
                )
            });
        check_deployment_info(shim, deployment_info);
        // check regular deployment receipts too
        assert_receipt(
            &shim.project,
            shim.meta.fqn.file.to_str().unwrap(),
            &shim.meta.fqn.name,
            &shim.address().or_bug("Address not set after deployment"),
        );
    }

    Ok(())
}

async fn deploy_and_test_api_low(cfg: &Config) -> Result<()> {
    async fn assert_retrieve<M: Middleware>(c: &EthersContract<M>, expected: U256) {
        let retrieve_call = c
            .method::<_, U256>("retrieve", ())
            .expect("retrieve method should exist");
        let actual = retrieve_call
            .call()
            .await
            .expect("retrieve call should succeed");
        assert_eq!(expected, actual);
    }

    println!("Deploying/testing ETH target");
    {
        let ethereum = TargetProjectInfo::new(cfg, Target::Ethereum)?
            .connect()
            .await?;

        let receiver_meta = ethereum.contract("StorageReceiver")?.unwrap();

        // try loading before deploying
        assert_matches!(
            ethereum.deployed(&receiver_meta).await,
            Err(CubistSdkError::LoadContractNoReceipts(..))
        );

        // deploy and check 'retrieve' reflects the value passed to the constructor
        let val = U256::from(42);
        let receiver = ethereum.deploy(&receiver_meta, val).await?;
        assert_retrieve(&receiver, val).await;

        // send 'store' and check 'retrieve' reflects is
        let val = U256::from(52);
        let store_call = receiver.method::<_, ()>("store", val)?;
        let tx = store_call.send().await?.await?;
        assert!(tx.is_some());
        assert_retrieve(&receiver, val).await;

        // assert deployment receipt
        assert_receipt(
            &ethereum,
            "StorageReceiver.sol",
            "StorageReceiver",
            &receiver.address().to_fixed_bytes(),
        );

        // test loading the contract by its address
        let receiver_2 = ethereum.at(&receiver_meta, receiver.address());
        assert_eq!(receiver.address(), receiver_2.address());
        assert_retrieve(&receiver, val).await;

        // test loading the contract by its deployment receipt
        let receiver_2 = ethereum.deployed(&receiver_meta).await?;
        assert_eq!(receiver.address(), receiver_2.address());
        assert_retrieve(&receiver, val).await;
    }

    println!("Deploying/testing Polygon target");
    {
        let polygon = TargetProjectInfo::new(cfg, Target::Polygon)?
            .connect()
            .await?;

        let receiver_meta = polygon.shim_contract("StorageReceiver")?.unwrap();
        let sender_meta = polygon.contract("StorageSender")?.unwrap();

        assert_matches!(
            polygon.deployed(&receiver_meta).await,
            Err(CubistSdkError::LoadContractNoReceipts(..))
        );
        assert_matches!(
            polygon.deployed(&sender_meta).await,
            Err(CubistSdkError::LoadContractNoReceipts(..))
        );

        // deploy and check 'retrieve' reflects the value passed to the constructor
        let val = U256::from(1337);
        let receiver = polygon.deploy(&receiver_meta, ()).await?;
        let sender = polygon
            .deploy(&sender_meta, (val, receiver.address()))
            .await?;
        assert_retrieve(&sender, val).await;

        // send 'store' and check 'retrieve' reflects is
        let val = U256::from(2337);

        // calling "store" on 'sender' will call "store" on 'receiver'
        // shim, which has not been approved yet, thus it should fail
        let store_call = sender.method::<_, ()>("store", val)?;
        let err = store_call.send().await.unwrap_err();
        let err_msg = format!("{err:?}");
        assert!(
            err_msg.contains("Cubist: sender is not a caller"),
            "Unexpected error message: '{err_msg}'"
        );

        // add 'sender' to approved callers of 'receiver' and try again
        receiver
            .method::<_, ()>(APPROVE_CALLER_METHOD_NAME, sender.address())?
            .send()
            .await?
            .await?;
        let store_call = sender.method::<_, ()>("store", val)?;
        let tx = store_call.send().await?.await?;
        assert!(tx.is_some());
        assert_retrieve(&sender, val).await;

        // assert deployment receipt
        assert_receipt(
            &polygon,
            "StorageReceiver.sol",
            "StorageReceiver",
            &receiver.address().to_fixed_bytes(),
        );
        assert_receipt(
            &polygon,
            "StorageSender.sol",
            "StorageSender",
            &sender.address().to_fixed_bytes(),
        );

        // test loading the contract by its address
        let sender_2 = polygon.at(&sender_meta, sender.address());
        assert_eq!(sender.address(), sender_2.address());
        assert_retrieve(&sender, val).await;

        // test loading the contract by its address
        let sender_2 = polygon.deployed(&sender_meta).await?;
        assert_eq!(sender.address(), sender_2.address());
        assert_retrieve(&sender, val).await;

        // deploy again and assert that loading from receits fails with too many receipts
        let val = U256::from(5337);
        let sender_2 = polygon
            .deploy(&sender_meta, (val, receiver.address()))
            .await?;
        assert_retrieve(&sender_2, val).await;
        assert_ne!(sender.address(), sender_2.address());
        assert_matches!(
            polygon.deployed(&sender_meta).await,
            Err(CubistSdkError::LoadContractTooManyReceipts(..))
        );
    }

    Ok(())
}

fn setup(name: &str, app_dir: &Path, type_: ProjType, template: Template, branch: &str) {
    cubist_cli::commands::new::from_template(
        name,
        type_,
        template,
        app_dir.parent().unwrap(),
        false,
        Some(branch.to_string()),
    )
    .context("Failed to setup test project")
    .unwrap();
}

async fn run_cubist_cmd(app_dir: PathBuf, cmd: &str) -> Result<()> {
    println!("cubist {cmd}");
    assert!(cubist_cmd()
        .arg(cmd)
        .current_dir(&app_dir)
        .status()
        .await?
        .success());
    Ok(())
}

fn cubist_cmd() -> Command {
    Command::new(cargo_bin("cubist"))
}

fn cargo_bin(name: &str) -> PathBuf {
    let env_var = format!("CARGO_BIN_EXE_{}", name);
    std::env::var_os(env_var)
        .map(|p| p.into())
        .unwrap_or_else(|| target_dir().join(format!("{}{}", name, env::consts::EXE_SUFFIX)))
}

fn target_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .map(|mut path| {
            path.pop();
            if path.ends_with("deps") {
                path.pop();
            }
            path
        })
        .unwrap()
}

/// A trivial contract that imports (and extends)
/// "@openzeppelin/contracts/token/ERC721/ERC721.sol".  Used as
/// regression test for auto-resolving contract imports to npm
/// packages (i.e., the "allow_import_from_external" config option)
#[tokio::test]
#[serial]
async fn openzeppelin_nft() -> Result<()> {
    let app_dir = project_fixture_dir("openzeppelin_nft");
    let cfg = Config::from_dir(&app_dir)?;
    let cfg_path = cfg.config_path.clone();

    println!("Building");
    clean(&app_dir);
    run_cubist_cmd(app_dir, "build").await?;

    defer! { stop_chains(cfg_path) }
    println!("Starting chains");
    start_chains(cfg.clone()).await?;

    let proj = Cubist::<Http>::new(cfg).await?;
    let game_item = proj.contract("GameItem").unwrap();

    println!("Deploying");
    game_item.deploy(()).await?;

    let name: String = game_item.call("name", ()).await?;
    let symbol: String = game_item.call("symbol", ()).await?;

    println!("Testing");
    assert_eq!(String::from("GameItem"), name);
    assert_eq!(String::from("ITM"), symbol);

    Ok(())
}

#[rstest]
#[case::eth_poly(Target::Ethereum, Target::Polygon)]
#[case::poly_eth(Target::Polygon, Target::Ethereum)]
#[case::ava_poly(Target::Avalanche, Target::Polygon)]
#[tokio::test]
#[serial]
async fn circular_foo_bar(#[case] foo_target: Target, #[case] bar_target: Target) -> Result<()> {
    let tmp = tempdir()?;
    let src_app_dir = project_fixture_dir("circular_imports");
    let app_dir = tmp.path().join("circular_imports");
    let mut opts = fs_extra::dir::CopyOptions::new();
    opts.copy_inside = true;
    fs_extra::dir::copy(&src_app_dir, &app_dir, &opts)?;
    let cfg_path = app_dir.join("cubist-config.json");
    let new_cfg_content = fs::read_to_string(&cfg_path)?
        .replace(r#""__FOO_TARGET__""#, &json!(foo_target).to_string())
        .replace(r#""__BAR_TARGET__""#, &json!(bar_target).to_string());
    fs::write(&cfg_path, new_cfg_content)?;
    let cfg = Config::from_dir(&app_dir)?;

    println!("Building");
    clean(&app_dir);
    run_cubist_cmd(app_dir, "build").await?;

    defer! { stop_chains(cfg_path) }
    println!("Starting chains");
    start_chains(cfg.clone()).await?;

    println!("Testing over WS");
    do_circular_foo_bar(Cubist::<Ws>::new(cfg.clone()).await?).await?;

    Ok(())
}

#[allow(clippy::disallowed_names)]
async fn do_circular_foo_bar<M: Middleware + 'static>(proj: Cubist<M>) -> Result<()> {
    let foo = proj.contract("Foo").unwrap();
    let bar = proj.contract("Bar").unwrap();

    println!("Deploying shims");
    foo.deploy_shims().await?;
    bar.deploy_shims().await?;

    println!("Deploying contracts");
    foo.deploy(Address::from_slice(&bar.address_on(foo.target())))
        .await?;
    bar.deploy(Address::from_slice(&foo.address_on(bar.target())))
        .await?;

    let mut relayer = create_relayer(&proj, 2);
    relayer.start().await?;

    println!("Testing 'store'");
    let foo_val = U256::from(1);
    let bar_val = U256::from(2);
    bar.send("call_foo", foo_val).await?;
    foo.send("call_bar", bar_val).await?;

    println!("Waiting until bridged");
    relayer.run_to_completion().await?;

    assert_eq!(foo_val, foo.call("retrieve", ()).await?);
    assert_eq!(bar_val, bar.call("retrieve", ()).await?);

    Ok(())
}

const CUBIST_TESTNET_MNEMONIC_ENV_VAR: &str = "CUBIST_TESTNET_MNEMONIC";

#[rstest]
#[case::eth_poly(Target::Ethereum, Target::Polygon)]
#[case::poly_eth(Target::Polygon, Target::Ethereum)]
// #[case::ava_poly(Target::Avalanche, Target::Polygon)]
// #[case::poly_ava(Target::Polygon, Target::Avalanche)]
#[tokio::test]
#[serial]
async fn counter_payable(#[case] from_target: Target, #[case] to_target: Target) -> Result<()> {
    let tmp = tempdir()?;
    let src_app_dir = project_fixture_dir("counter_payable");
    let app_dir = tmp.path().join(src_app_dir.file_name().unwrap());
    let mut opts = fs_extra::dir::CopyOptions::new();
    opts.copy_inside = true;
    fs_extra::dir::copy(&src_app_dir, &app_dir, &opts)?;
    let cfg_path = app_dir.join("cubist-config.json");
    let new_cfg_content = fs::read_to_string(&cfg_path)?
        .replace(r#""__FROM_TARGET__""#, &json!(from_target).to_string())
        .replace(r#""__TO_TARGET__""#, &json!(to_target).to_string());
    fs::write(&cfg_path, new_cfg_content)?;
    let cfg = Config::from_dir(&app_dir)?;

    println!("Building");
    run_cubist_cmd(app_dir, "build").await?;

    defer! { stop_services(cfg_path) }
    println!("Starting");
    start_services(&cfg).await?;

    let proj = Cubist::<Http>::new(cfg).await?;
    let from = proj.contract("From").unwrap();
    let to = proj.contract("To").unwrap();

    println!("Deploying contracts");
    to.deploy(()).await?;
    from.deploy(Address::from_slice(&to.address_on(from.target())))
        .await?;

    for val in [U256::from(123), U256::from(456)] {
        println!("Testing 'store({val})'");
        let mut call = from.method::<_, ()>("store", val)?;
        call.tx.set_value(5_000_000u64); // arbitrary amount big enough to pay for axelar fees
        call.send().await?.await?;

        assert_eq!(val, from.call("retrieve", ()).await?);

        for d in repeat(Duration::from_millis(200)).take(50) {
            let to_val: U256 = to.call("retrieve", ()).await?;
            if to_val == val {
                break;
            } else {
                tokio::time::sleep(d).await;
            }
        }

        assert_eq!(val, to.call("retrieve", ()).await?);
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn counter_testnets() -> Result<()> {
    let app_dir = project_fixture_dir("counter");
    let cfg = Config::from_dir(&app_dir)?;
    let cfg_path = cfg.config_path.clone();

    if dotenv::var(CUBIST_TESTNET_MNEMONIC_ENV_VAR).is_err() {
        println!(
            "{}: Set {} env var to run this test",
            "*** WARN ***".bright_yellow(),
            CUBIST_TESTNET_MNEMONIC_ENV_VAR.bright_blue()
        );
        return Ok(());
    }

    println!("Building");
    clean(&app_dir);
    run_cubist_cmd(app_dir, "build").await?;

    defer! { stop_chains(cfg_path) }
    println!("Starting chains");
    start_chains(cfg.clone()).await?;

    let proj = Cubist::<Ws>::new(cfg).await?;
    let from = proj.contract("From").unwrap();
    let to = proj.contract("To").unwrap();

    println!("Deploying contracts");
    to.deploy(()).await?;
    from.deploy(Address::from_slice(&to.address_on(from.target())))
        .await?;

    let mut relayer = create_relayer(&proj, 1);
    relayer.start().await?;

    println!("Testing 'store'");
    let val = U256::from(123);
    from.send("store", val).await?;

    println!("Waiting until bridged");
    relayer.run_to_completion().await?;

    assert_eq!(val, from.call("retrieve", ()).await?);
    assert_eq!(val, to.call("retrieve", ()).await?);

    Ok(())
}

#[tokio::test]
#[serial]
async fn mpmc() -> Result<()> {
    let tmp = tempdir()?;
    let name = "mpmc_app".to_string();
    let app_dir = tmp.path().join(&name);

    // run 'new --template MPMC --type Rust'
    // this template has the following contracts:
    //
    //   S1 ───> Channel ───> R1
    //   S2 ___↗        `───> R2
    //
    setup(&name, &app_dir, ProjType::Rust, Template::MPMC, TEST_BRANCH);

    // load (and validate) the config
    let cfg = Config::from_dir(&app_dir)?;
    let cfg_path = cfg.config_path.clone();

    // start chains and run 'cubist build' in parallel
    defer! { stop_chains(cfg_path) }
    println!("Starting chains");
    try_join!(start_chains(cfg.clone()), run_cubist_cmd(app_dir, "build"))?;

    // deploy and test
    let proj = Cubist::<Http>::new(cfg.clone()).await?;
    let s1 = proj.contract("S1").unwrap();
    let s2 = proj.contract("S2").unwrap();
    let ch = proj.contract("Channel").unwrap();
    let r1 = proj.contract("R1").unwrap();
    let r2 = proj.contract("R2").unwrap();

    // deploy receivers, then channel, then senders
    println!("Deploying");
    r1.deploy(()).await?;
    r2.deploy(()).await?;
    ch.deploy((
        Address::from_slice(&r1.address_on(ch.target())),
        Address::from_slice(&r2.address_on(ch.target())),
    ))
    .await?;
    s1.deploy(Address::from_slice(&ch.address_on(s1.target())))
        .await?;
    s2.deploy(Address::from_slice(&ch.address_on(s2.target())))
        .await?;

    // start the bridge right away and tell it to process 6 events:
    // from {S1, S2} to CH, 2x from CH to R1, 2x from CH to R2
    let mut relayer = create_relayer(&proj, 6);
    relayer.start().await?;

    // call s1.send(val1) and s2.send(val2)
    println!("Testing 'send'");
    let val1 = U256::from(1);
    let val2 = U256::from(2);
    s1.send("send", val1).await?;
    s2.send("send", val2).await?;

    // wait for the bridge to process all 6 events
    relayer.run_to_completion().await?;

    // assert both receivers have updated values (either val1 or val2 because there is no guarantee
    // which of two "send" calls above (one from Avalanche and one from Ethereum) will be relayed first)
    let (r1val, r2val): (U256, U256) = try_join!(r1.call("retrieve", ()), r2.call("retrieve", ()))?;
    assert!(r1val == val1 || r1val == val2);
    assert!(r2val == val1 || r2val == val2);

    Ok(())
}

#[rstest]
#[case::low(ApiKind::Low)]
#[case::mid(ApiKind::Mid)]
#[case::orm(ApiKind::Orm)]
#[tokio::test]
#[serial]
async fn storage(#[case] api_kind: ApiKind) -> Result<()> {
    let tmp = tempdir()?;
    let name = "my_app".to_string();
    let app_dir = tmp.path().join(&name);

    // run 'new'
    setup(
        &name,
        &app_dir,
        ProjType::Rust,
        Template::Storage,
        TEST_BRANCH,
    );

    // load (and validate) the config
    let cfg = Config::from_dir(&app_dir)?;
    let cfg_path = cfg.config_path.clone();

    // start chains (it takes some time until the chains are capable of accepting connections)
    defer! { stop_chains(cfg_path) }
    println!("Starting chains");
    start_chains(cfg.clone()).await?;

    // run 'cubist build' to generate contract shims and compile all contracts
    run_cubist_cmd(app_dir, "build").await?;

    // deploy and run basic validations
    println!("=== Testing SDK");
    match api_kind {
        ApiKind::Low => deploy_and_test_api_low(&cfg).await?,
        ApiKind::Mid => deploy_and_test_api_mid(cfg).await?,
        ApiKind::Orm => deploy_and_test_orm_api(&cfg).await?,
    };

    println!("DONE");
    Ok(())
}
