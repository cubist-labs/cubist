use assert_cmd::prelude::*;
use cubist_cli::cube::template::Template;
use cubist_config::{Config, ProjType};
use rstest::rstest;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[rstest]
#[case::java_script(ProjType::JavaScript)]
#[case::rust(ProjType::Rust)]
#[case::type_script(ProjType::TypeScript)]
fn empty(#[case] proj_type: ProjType) {
    let tmp = tempdir().unwrap();
    let name = "myApp".to_string();

    let mut cmd = Command::cargo_bin("cubist").unwrap();
    cmd.args([
        "new",
        "--type",
        proj_type.to_string().as_str(),
        "--dir",
        &tmp.path().to_string_lossy(),
        &name,
    ])
    .assert()
    .success();

    sanity_check(&tmp.path().join(&name), &name, proj_type);
}

#[test]
fn js_from_git_repo() {
    let tmp = tempdir().unwrap();
    let name = "myApp".to_string();

    let mut cmd = Command::cargo_bin("cubist").unwrap();
    cmd.args([
        "new",
        "--dir",
        &tmp.path().to_string_lossy(),
        "--from-repo",
        "git@github.com:cubist-alpha/test-dummy-sdk-project.git",
        &name,
    ])
    .assert()
    .success();
    sanity_check_js(&tmp.path().join(&name), "test-dummy-sdk-project", &|_| ());
}

#[rstest]
fn from_template(
    #[values(Template::Storage, Template::MPMC, Template::TokenBridge)] template: Template,
    #[values(ProjType::JavaScript, ProjType::Rust, ProjType::TypeScript)] proj_type: ProjType,
) {
    let tmp = tempdir().unwrap();
    let name = "myApp".to_string();

    let mut cmd = Command::cargo_bin("cubist").unwrap();
    cmd.args([
        "new",
        "--type",
        &proj_type.to_string(),
        "--dir",
        &tmp.path().to_string_lossy(),
        "--template",
        &template.to_string(),
        &name,
    ])
    .assert()
    .success();
    sanity_check(&tmp.path().join(&name), &name, proj_type);
}

fn sanity_check(dir: &Path, app_name: &str, proj_type: ProjType) {
    // sanity check cubist-config.json
    sanity_check_cubist(dir, proj_type);

    // project-specific sanity checks
    match proj_type {
        ProjType::JavaScript => sanity_check_js(dir, app_name, &|_| ()),
        ProjType::Rust => sanity_check_rs(dir, app_name, &|_| ()),
        ProjType::TypeScript => sanity_check_ts(dir, app_name, &|_| ()),
    }
}

fn sanity_check_cubist(dir: &Path, proj_type: ProjType) {
    assert!(dir.is_dir(), "Project directory exists");

    // sanity check cubist config
    let config_file = dir.join("cubist-config.json");
    assert!(config_file.is_file(), "cubist-config.json exists");
    let cfg = Config::from_file(config_file).unwrap();
    assert_eq!(cfg.type_, proj_type);
}

fn sanity_check_js(dir: &Path, app_name: &str, custom: &dyn Fn(Value)) {
    // sanity check package.json
    let package_file = dir.join("package.json");
    assert!(package_file.is_file(), "package.json exists");
    let contents = fs::read_to_string(package_file).unwrap();
    let pkg: Value = serde_json::from_str(&contents).unwrap();
    assert!(pkg["name"].is_string());
    assert_eq!(pkg["name"], json!(app_name));
    assert!(pkg["version"].is_string());
    assert!(pkg["description"].is_string());
    assert!(pkg["license"].is_string());
    assert!(pkg["dependencies"].is_object());
    assert!(pkg["dependencies"]
        .as_object()
        .unwrap()
        .contains_key("@cubist-alpha/cubist"));
    custom(pkg);
}

fn sanity_check_ts(dir: &Path, app_name: &str, custom: &dyn Fn(Value)) {
    let tsconfig_file = dir.join("tsconfig.json");
    assert!(tsconfig_file.is_file(), "tsconfig.json exists");
    sanity_check_js(dir, app_name, custom);
}

fn sanity_check_rs(dir: &Path, app_name: &str, custom: &dyn Fn(toml::Value)) {
    // sanity check package.json
    let cargo_file = dir.join("Cargo.toml");
    assert!(cargo_file.is_file(), "Cargo.toml exists");
    let contents = fs::read_to_string(cargo_file).unwrap();
    let tml: toml::Value = toml::from_str(&contents).unwrap();
    assert_toml_str(&tml, vec!["package", "name"], app_name);
    assert_toml_str(&tml, vec!["package", "version"], "0.1.0");
    assert_toml_str(&tml, vec!["package", "edition"], "2021");
    assert_toml_str(
        &tml,
        vec!["dependencies", "cubist-sdk", "git"],
        "ssh://git@github.com/cubist-alpha/cubist.git",
    );
    assert_toml_str(
        &tml,
        vec!["dependencies", "cubist-sdk", "package"],
        "cubist-sdk",
    );
    assert_toml_str(
        &tml,
        vec!["dependencies", "cubist-config", "package"],
        "cubist-config",
    );
    custom(tml);
}

fn assert_toml_str(tml: &toml::Value, query: Vec<&str>, expected: &str) {
    assert_eq!(
        Some(toml::Value::String(expected.to_string())),
        query
            .iter()
            .fold(Some(tml), |acc, p_name| acc.and_then(|v| v.get(p_name)))
            .map(|v| v.to_owned())
    );
}
