use cubist_config::network::{CredConfig, KeystoreConfig, MnemonicConfig};
use cubist_config::*;
use path_clean::PathClean;
use pretty_assertions::assert_eq;
use url::Url;

use std::env;
use std::path::PathBuf;

#[test]
fn test_good_config() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    // create config
    let cfg = Config::from_file(dir.join("good-config.json")).unwrap();
    assert_eq!(cfg.type_, ProjType::JavaScript);
    assert_eq!(cfg.build_dir(), dir.as_path().join("build_dir").clean());
    assert_eq!(
        cfg.deploy_dir(),
        dir.as_path().join("../deploy_dir").clean()
    );
    let contracts = cfg.contracts();
    assert_eq!(contracts.root_dir, dir.as_path().join("contracts").clean());

    let default_profile = &cfg.network_profiles["default"];
    assert_eq!(
        default_profile
            .avalanche
            .as_ref()
            .unwrap()
            .common
            .url
            .to_string(),
        "http://localhost:9560/"
    );
    assert_eq!(
        default_profile
            .polygon
            .as_ref()
            .unwrap()
            .common
            .url
            .to_string(),
        "http://localhost:9545/"
    );
    assert_eq!(
        default_profile
            .ethereum
            .as_ref()
            .unwrap()
            .common
            .url
            .to_string(),
        "http://otherhost:7545/"
    );

    let dev_profile = &cfg.network_profiles["dev"];
    assert_eq!(
        dev_profile
            .avalanche
            .as_ref()
            .unwrap()
            .common
            .url
            .to_string(),
        "http://otherhost:9560/"
    );
    assert_eq!(
        dev_profile.avalanche.as_ref().unwrap().common.autostart,
        false
    );
    assert_eq!(
        dev_profile.polygon.as_ref().unwrap().common.url.to_string(),
        "http://localhost:9545/"
    );
    assert_eq!(
        dev_profile
            .ethereum
            .as_ref()
            .unwrap()
            .common
            .url
            .to_string(),
        "http://localhost:7545/"
    );

    let testnet_profile = &cfg.network_profiles["testnets"];
    let polygon_config = testnet_profile.polygon.as_ref().unwrap();
    assert_eq!(
        polygon_config.common.url.expose_url().unwrap(),
        Url::parse("https://rpc-mumbai.maticvigil.com").unwrap()
    );
    assert!(polygon_config.common.proxy.is_some());
    let proxy = polygon_config.common.proxy.as_ref().unwrap();
    assert_eq!(proxy.port, 9545);
    assert_eq!(proxy.chain_id, 80001);
    assert_eq!(proxy.creds.len(), 3);
    assert!(matches!(
        proxy.creds.first(),
        Some(CredConfig::Mnemonic(MnemonicConfig {
            account_count: 2,
            ..
        }))
    ));
    assert!(matches!(
        proxy.creds.get(1),
        Some(CredConfig::Keystore(KeystoreConfig { .. }))
    ));
    assert!(matches!(
        proxy.creds.get(2),
        Some(CredConfig::PrivateKey(..))
    ));
}

#[test]
fn test_bad_config_compiler() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    // create config
    match Config::from_file(dir.join("bad-config-compiler.json")) {
        Err(ConfigError::MalformedConfig(..)) => (),
        c => panic!("Expected error, got {:?}", c),
    }
}

#[test]
fn test_bad_config_project() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    // create config
    match Config::from_file(dir.join("bad-config-project.json")) {
        Err(ConfigError::MalformedConfig(..)) => (),
        c => panic!("Expected error, got {:?}", c),
    }
}

#[test]
fn test_bad_config_paths() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    // create config
    match Config::from_file(dir.join("bad-config-paths.json")) {
        Err(ConfigError::InvalidContractFilePaths(mut paths)) => {
            assert_eq!(
                paths.sort(),
                [
                    dir.as_path().join("./contracts/test/ava.sol").clean(),
                    dir.as_path().join("./contracts/poly.sol").clean(),
                ]
                .sort()
            );
        }
        c => panic!("Expected error, got {:?}", c),
    }
}

#[test]
fn test_bad_config_missing_network_profile() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    // create config
    match Config::from_file(dir.join("bad-config-missing-network-profile.json")) {
        Err(ConfigError::MissingNetworkProfile(profile)) => assert_eq!(profile, "missingprofile"),
        c => panic!("Expected error, got {:?}", c),
    }
}

#[test]
fn test_nearest() {
    // tests the cubist-config.json in fixtures
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    assert!(env::set_current_dir(&dir).is_ok());

    // Load config from disk
    let cfg = Config::nearest().unwrap();
    assert_eq!(cfg.type_, ProjType::Rust);
    assert_eq!(cfg.build_dir(), dir.as_path().join("build").clean());
    assert_eq!(cfg.deploy_dir(), dir.as_path().join("deploy").clean());
    let contracts = cfg.contracts();
    assert_eq!(contracts.root_dir, dir.as_path().join("contracts").clean());
}
