use cubist_config::Config;
use cubist_sdk::parse::SourceFiles;
use cubist_sdk::CubistSdkError;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

macro_rules! import_error_test {
    ($config_path: expr, $pattern:pat) => {{
        let config = Config::from_file($config_path).unwrap();
        let source_files = SourceFiles::new(config.contracts()).expect("Error parsing files");
        assert!(source_files.sources.len() == 1);
        let result = source_files.sources.first().unwrap().check_imports(&config);
        println!("Result is: {:#?}", result);
        assert!(matches!(result, Err($pattern)));
    }};
}

fn setup_proj(tmp: &Path, contract: &str) -> PathBuf {
    fs::create_dir_all(tmp).unwrap();
    let app_dir = tmp.join("my-app");
    fs::create_dir_all(app_dir.clone()).unwrap();
    let config_path = app_dir.join("cubist-config.json");
    assert!(fs::write(config_path.clone(), CONFIG).is_ok());
    println!("Created config at {}", config_path.display());

    let contracts_dir = app_dir.join("contracts");
    assert!(fs::create_dir_all(contracts_dir.clone()).is_ok());
    let contract_file = contracts_dir.join("Error.sol");
    assert!(fs::write(contract_file.clone(), contract).is_ok());
    println!("Created contract at {}", contract_file.display());

    config_path
}

static CONFIG: &str = r#"
{
    "type": "JavaScript",
    "contracts": {
        "root_dir": "contracts/",
        "targets": {
          "avalanche": { "files": [ "contracts/Error.sol" ] }
        }
    }
}
"#;

static CONTRACT: &str = r#"
    // SPDX-License-Identifier: MIT
    pragma solidity ^0.8.17;

    import '../Dummy.sol';
"#;

#[test]
fn canonicalization_error() {
    let tmp = tempdir().unwrap().into_path();
    let config_path = setup_proj(&tmp, CONTRACT);
    // Don't create the imported file ../Dummy.sol
    // Make sure we get a canonicalization error
    import_error_test!(config_path, CubistSdkError::CanonicalizationError(..));
}

#[test]
fn relative_path_error() {
    let tmp = tempdir().unwrap().into_path();
    let config_path = setup_proj(&tmp, CONTRACT);
    // Create the imported file
    let dummy_file = tmp.join("my-app").join("Dummy.sol");
    assert!(fs::write(dummy_file, "").is_ok());
    // Make sure we get a relative import error
    import_error_test!(config_path, CubistSdkError::RelativePathError(..));
}

#[test]
fn absolute_path_error() {
    let tmp = tempdir().unwrap().into_path();
    let contract = format!(
        r#"
    // SPDX-License-Identifier: MIT
    pragma solidity ^0.8.17;

    import '{}/my-app/contracts/EthStorage.sol';
    "#,
        tmp.display()
    );
    println!("Contract: {contract}");
    let config_path = setup_proj(&tmp, &contract);
    // Create the imported file
    let dummy_file = tmp.join("my-app/contracts/EthStorage.sol");
    assert!(fs::write(dummy_file, "").is_ok());
    import_error_test!(config_path, CubistSdkError::AbsolutePathError(..))
}
