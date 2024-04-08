use cubist_config::{Config, Target};
use cubist_sdk::gen::backend::{AxelarBackend, AxelarNetwork, Backend, CubistBackend};
use cubist_sdk::gen::common::InterfaceGenError;
use cubist_sdk::gen::interface::{get_interface_for_contract, Interfaces};
use cubist_sdk::parse::parse_files;
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::Value;
use std::collections::HashSet as Set;
use std::fs;
use std::path::{Path, PathBuf};
use tempdir::TempDir;
use walkdir::WalkDir;

/// Returns the path to the `tests/code` directory
fn code_path() -> PathBuf {
    let test_path = env!("CARGO_MANIFEST_DIR").to_string();
    Path::new(&test_path).join("tests/code")
}

/// Generating interfaces for the files listed in the config should result in an error
macro_rules! error_test_case {
    ($config_file: expr, $pattern:pat) => {{
        let config_path = code_path().join($config_file);
        let maybe_config = Config::from_file(config_path);
        if let Ok(config) = maybe_config {
            let source_files = parse_files(&config.contracts()).expect("Error parsing files");
            let result = Interfaces::new(&source_files);
            assert!(matches!(result, Err($pattern)));
        }
    }};
}

// Utilities for testing generated files (that we put in temporary directories)
// against their corresponding oracle files

/// Generating interfaces for the files listed in the config should result in the exact contents of
/// expected_output_dir. There is a subdirectory for each tested back end.
fn test_case(config_file: &str, expected_output_dir: &str) {
    let expected_output_path = code_path().join(expected_output_dir);
    assert!(
        expected_output_path.is_dir(),
        "Expected output dir {} is not a directory",
        expected_output_path.display()
    );
    let output_dir = TempDir::new("out").expect("Temporary directory creation failed");
    let output_path = output_dir.path();
    make_test_case(config_file, output_path);
    check_test_case(output_path, &expected_output_path);
}

/// Genreate an interface that exposes the specified contract from a file.
fn contract_test_case(
    config_file: &str,
    contract: &str,
    expected_output_dir: &str,
    shim_targets: &Set<Target>,
) {
    let expected_output_path = code_path().join(expected_output_dir);
    let output_dir = TempDir::new("out").expect("Temporary directory creation failed");
    let output_path = output_dir.path();
    make_contract_test_case(config_file, contract, output_path, shim_targets);
    check_test_case(output_path, &expected_output_path);
}

/// Make sure the generated files match the oracle files
fn check_test_case(output_path: &Path, expected_output_path: &Path) {
    let read_non_empty_lines = |file_path: &PathBuf| {
        fs::read_to_string(file_path)
            .unwrap_or_else(|_| panic!("Unable to read file {}", file_path.display()))
            .lines()
            .filter(|line| !line.is_empty())
            .collect::<Vec<&str>>()
            .join("\n")
    };

    let expected_num = WalkDir::new(expected_output_path).into_iter().count();
    let actual_num = WalkDir::new(output_path).into_iter().count();
    assert_eq!(
        expected_num, actual_num,
        "Expected {} files, got {}",
        expected_num, actual_num
    );

    // Check to make sure that the expected results and the actual results are the same.
    let backend_paths = fs::read_dir(output_path).unwrap();
    for backend_path in backend_paths {
        let out_backend_path = backend_path.expect("Expected folder").path();
        assert!(
            out_backend_path.is_dir(),
            "Expected {} to be a directory",
            out_backend_path.display()
        );

        let backend_path_comp = out_backend_path.components().last().unwrap();
        let chain_paths = fs::read_dir(&out_backend_path).unwrap();
        for chain_path in chain_paths {
            let out_backend_chain_path = chain_path.expect("Expected folder").path();
            assert!(
                out_backend_chain_path.is_dir(),
                "Expected {} to be a directory",
                out_backend_chain_path.display()
            );

            let chain_path_comp = out_backend_chain_path.components().last().unwrap();
            let files = fs::read_dir(&out_backend_chain_path).unwrap();
            for f in files {
                let out_path = f.expect("Expected file");
                if !out_path.path().is_file() {
                    continue;
                }
                let expected_path = expected_output_path
                    .join(backend_path_comp)
                    .join(chain_path_comp)
                    .join(out_path.file_name());
                assert!(
                    expected_path.exists(),
                    "Unexpected file {}",
                    out_path.path().display()
                );

                if out_path.path().extension().unwrap() == "json" {
                    fn read_json(path: &Path) -> Value {
                        let content = fs::read_to_string(path).expect("Could not read file");
                        serde_json::from_str::<Value>(&content).expect("Could not parse JSON file")
                    }

                    // For JSON files, we parse the files as untyped JSON values
                    // and compare them. This ensures that whitespace differences
                    // are ignored.
                    let out_json: Value = read_json(&out_path.path());
                    let expected_json: Value = read_json(&expected_path);
                    assert_eq!(
                        out_json,
                        expected_json,
                        "JSON content {} does not match",
                        out_path.path().display()
                    );
                } else {
                    let out_source = read_non_empty_lines(&out_path.path());
                    // // UNCOMMENT to auto-fix oracles
                    // if out_source != read_non_empty_lines(&expected_path) {
                    //     fs::write(&expected_path, &out_source).unwrap();
                    // }
                    let expected_source = read_non_empty_lines(&expected_path);
                    assert_eq!(
                        out_source,
                        expected_source,
                        "File {} does not match {}",
                        out_path.path().display(),
                        expected_path.display(),
                    );
                }
            }
        }
    }
}

/// Generate interfaces for everything listed in the config, and then
/// actually write those contract interfaces to files in the output_dir
/// NOTE: This function will write *all interfaces,* not just the ones
/// specified in the config. Ie, currently it will write the Cubist
/// sender and bridge files, and the Axelar sender and receiver interfaces
pub fn write_all_interfaces(interfaces: &Interfaces, output_path: &Path) {
    for interface in &interfaces.interfaces {
        // Write Axelar and Cubist interfaces and bridge files
        let gens: Vec<Box<dyn Backend>> = vec![
            Box::new(AxelarBackend::new(AxelarNetwork::Localnet)),
            Box::new(CubistBackend),
        ];
        for gen in gens {
            let artifacts = gen.process(interface).expect("Error in backend");
            let backend_output_path = output_path.join(gen.name());
            for artifact in artifacts {
                let chain_path = backend_output_path.join(artifact.target());
                // Ensure that the directory for the target chain exists
                fs::create_dir_all(&chain_path).expect("Could not create directory");
                let file_name = chain_path.join(artifact.name());
                // Replace the absolute path.
                // We're doing this instead of generating relative paths as a
                // way of (hopefully) more faithfully testing our import re-writing.
                let to_replace = code_path().display().to_string();
                let file_content = artifact.content().as_str().replace(&to_replace, ".");
                fs::create_dir_all(file_name.parent().unwrap())
                    .expect("Could not create parent dir");
                fs::write(&file_name, file_content).expect("Could not write file");
                println!("Generated {}", file_name.display());
            }
        }
    }
}

// These functions are split out because they're useful on their own
// for generating the oracle files

/// Generate interfaces for everything listed in the config, and make sure they parse
fn make_test_case(config_file: &str, output_path: &Path) {
    // Generate interfaces and write the results to output_dir
    let config_path = code_path().join(config_file);
    let config = Config::from_file(config_path).unwrap();
    let source_files = parse_files(config.contracts()).expect("Error parsing files");
    let interfaces = Interfaces::new(&source_files).unwrap();
    write_all_interfaces(&interfaces, output_path);

    // Check that the generated interfaces actually parse
    for dir_entry in fs::read_dir(output_path).unwrap() {
        let path = dir_entry.expect("Directory entry failed").path();
        if path.is_file() && path.extension().unwrap() == "sol" {
            let code = fs::read_to_string(path).expect("Unable to open file");
            println!("{}", code);
            assert!(solang_parser::parse(&code, 0).is_ok());
        }
    }
}

/// Use get_all_interfaces to get interfaces for every exposible function in
/// [`contract`], which is within [`input`]. Output the resulting shim to
/// [`output_dir`]. This function assigns the input and resulting shim to
/// dummy targets, since the target isn't important to the logic of get_all_interfaces
/// (unlike in get_interfaces, which has to figure out which shims go where,
/// get_all_interfaces takes the shim targets as an input)
fn make_contract_test_case(
    config_file: &str,
    contract: &str,
    output_path: &Path,
    shim_targets: &Set<Target>,
) {
    let config_path = code_path().join(config_file);
    let config = Config::from_file(config_path).unwrap();
    let source_files = parse_files(config.contracts()).expect("Error parsing files");
    let interfaces =
        get_interface_for_contract(&source_files, &contract.to_string(), shim_targets).unwrap();
    write_all_interfaces(&interfaces, output_path);
}

#[test]
fn marketplace_all() {
    contract_test_case(
        "marketplace-all/config.json",
        "Marketplace",
        "marketplace-all/out",
        &Set::from([Target::Ethereum, Target::Polygon]),
    );
}

#[test]
fn public_getter() {
    error_test_case!(
        "public-getter/config.json",
        InterfaceGenError::MissingFunction(_)
    )
}

#[test]
fn private_function() {
    error_test_case!(
        "private-function/config.json",
        InterfaceGenError::GenerateInterfaceError(_)
    )
}

#[test]
fn return_value() {
    error_test_case!(
        "return-value/config.json",
        InterfaceGenError::GenerateInterfaceError(_)
    )
}

#[test]
fn bad_config() {
    error_test_case!(
        "bad-config/config.json",
        InterfaceGenError::DuplicateContracts(_)
    );
}

#[rstest]
#[case::ava_eth("ava-eth")]
#[case::ava_eth_payable("ava-eth-payable")]
#[case::ava_eth_poly("ava-eth-poly")]
#[case::enum_forward("enum-forward")]
#[case::struct_forward("struct-forward")]
#[case::license("license")]
#[case::import_forward("import-forward")]
#[case::basic_alias("basic-alias")]
#[case::rename_alias("rename-alias")]
#[case::star_alias("star-alias")]
#[case::raffle("raffle")]
#[case::charity_raffle("charity-raffle")]
#[case::nft_flower("nft-flower")]
#[case::only_owner("only-owner")]
#[case::marketplace("marketplace")]
#[case::token_bridge("token-bridge")]
fn test_cases(#[case] name: &str) {
    test_case(&format!("{name}/config.json"), &format!("{name}/out"));
}
