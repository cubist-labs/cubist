use cubist_config::*;

use path_clean::PathClean;
use serial_test::serial;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

// These tests set environment variables, so must run serially and in a separate process (hence why
// they're not unit tests). We do this to test configurations that are overridden through the
// environment.

#[test]
#[serial]
fn test_merge_env() {
    let tmp = tempdir().unwrap();
    let file_path = tmp.path().join(DEFAULT_FILENAME);
    fs::write(
        &file_path,
        r#" { "type": "Rust", "network_profiles": { "dev": {} } } "#,
    )
    .unwrap();

    // Set environment variables for build and deploy dirs
    std::env::set_var("CUBIST_BUILD_DIR", "/build_dir_env");
    std::env::set_var("CUBIST_DEPLOY_DIR", "/tmp/deploy_dir_env");
    std::env::set_var("CUBIST_NETWORK_PROFILE", "dev");
    // Create config
    let cfg = Config::from_dir(tmp.path()).unwrap();
    // Clear environment variables for build and deploy dirs
    std::env::remove_var("CUBIST_BUILD_DIR");
    std::env::remove_var("CUBIST_DEPLOY_DIR");
    std::env::remove_var("CUBIST_NETWORK_PROFILE");
    // Check:
    assert_eq!(cfg.type_, ProjType::Rust);
    assert_eq!(cfg.build_dir(), Path::new("/build_dir_env"));
    assert_eq!(cfg.deploy_dir(), Path::new("/tmp/deploy_dir_env"));
    assert_eq!(cfg.current_network_profile, "dev");
    let contracts = cfg.contracts();
    assert_eq!(contracts.root_dir, tmp.path().join("contracts").clean());
    assert!(contracts.targets.is_empty());
}
