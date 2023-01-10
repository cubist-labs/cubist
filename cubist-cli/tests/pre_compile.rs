use cubist_cli::commands::pre_compile::PreCompiler;
use cubist_config::Config;
use eyre::Result;
use std::{env, path::Path};

#[test]
fn test_validation_failure() -> Result<()> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR").to_string();
    let config_file = Path::new(&manifest_dir).join("tests/fixtures/projects/validation_failure");
    let cfg = Config::from_dir(&config_file)?;
    assert!(matches!(PreCompiler::new(&cfg), Err(_)));
    Ok(())
}
