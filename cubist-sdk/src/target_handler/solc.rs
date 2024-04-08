use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use crate::core::{CompileResult, ContractCompiler, ContractData, ContractInfo};
use crate::{CubistSdkError, Result, WrapperError};
use cubist_config::paths::{ContractFQN, TargetPaths};
use cubist_config::util::OrBug;
use cubist_config::CompilerConfig;
use ethers::core::abi::Abi;
use ethers::core::types::Bytes;
use ethers_solc::artifacts::Severity;
use ethers_solc::remappings::Remapping;
use ethers_solc::{Project, ProjectPathsConfig};

pub struct SolcCompiler {
    project: Project,
}

impl SolcCompiler {
    /// Creates a new instance of the solc compiler
    pub fn new(compiler_config: &CompilerConfig, paths: &TargetPaths) -> Self {
        Self {
            project: configure_solc_project(compiler_config, paths),
        }
    }

    /// Creates a new instance of the solc compiler that can serve as a validator
    pub fn new_validator(compiler_config: &CompilerConfig) -> Self {
        let project = Project::builder()
            .no_artifacts()
            .include_paths(compiler_config.import_dirs.clone())
            .build()
            .unwrap();
        SolcCompiler { project }
    }
}

fn parse_contract(json_path: &Path) -> Result<(Abi, Bytes)> {
    let invalid_contract = |reason: &str, source: Option<Box<WrapperError>>| {
        CubistSdkError::ParseContractError(json_path.to_path_buf(), reason.into(), source)
    };

    let json = fs::read_to_string(json_path)
        .map_err(|e| WrapperError::IOError(json_path.to_path_buf(), e))
        .map_err(|e| invalid_contract("Read error", Some(Box::new(e))))?;

    let val: serde_json::Value = serde_json::from_str(json.as_str())
        .map_err(|e| {
            WrapperError::JsonError(json_path.to_path_buf(), "CompileArtifact".to_owned(), e)
        })
        .map_err(|e| invalid_contract("Malformed JSON", Some(Box::new(e))))?;

    let abi = val
        .get("abi")
        .ok_or_else(|| invalid_contract("Property 'abi' not found", None))?;

    let abi: Abi = serde_json::from_value(abi.to_owned())
        .map_err(|e| WrapperError::JsonError(json_path.to_path_buf(), "Abi".to_owned(), e))
        .map_err(|e| invalid_contract("Invalid 'abi' value", Some(Box::new(e))))?;

    let bytes = val
        .get("bytecode")
        .and_then(|v| v.get("object"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            invalid_contract(
                "Property 'bytecode.object' not found or is not a string",
                None,
            )
        })?;
    let bytes = Bytes::from_str(bytes)
        .map_err(WrapperError::ParseBytesError)
        .map_err(|e| {
            invalid_contract(
                "Property 'bytecode.object' is not a hex string",
                Some(Box::new(e)),
            )
        })?;

    Ok((abi, bytes))
}

impl ContractCompiler for SolcCompiler {
    fn clean(&self) -> Result<()> {
        for dir in [
            self.project.artifacts_path(),
            self.project.build_info_path(),
        ] {
            if dir.is_dir() {
                fs::remove_dir_all(dir)
                    .map_err(|e| WrapperError::IOError(dir.clone(), e))
                    .map_err(|e| CubistSdkError::CleanError(dir.clone(), Box::new(e)))?
            }
        }
        Ok(())
    }

    fn compile_file(&self, file: &Path) -> Result<CompileResult> {
        let out = self
            .project
            .compile_file(file)
            .map_err(WrapperError::SolcError)
            .map_err(|e| {
                CubistSdkError::CompileError(
                    file.into(),
                    "'solc' invocation failed".into(),
                    Some(Box::new(e)),
                )
            })?;

        let has_errors = out.has_compiler_errors();
        let diagnostics = out.output().diagnostics(&[], Severity::Info).to_string();
        match has_errors {
            true => Err(CubistSdkError::CompileError(file.into(), diagnostics, None)),
            false => Ok(CompileResult { diagnostics }),
        }
    }

    fn find_compiled_contracts(&self, source_file: &Path) -> Result<HashMap<String, ContractInfo>> {
        let file_name = source_file.file_name().or_bug("File must have a name");
        let artifacts_dir = self.project.artifacts_path().join(file_name);
        let read_dir_result = std::fs::read_dir(&artifacts_dir)
            .map_err(|e| WrapperError::IOError(artifacts_dir.clone(), e))
            .map_err(|e| CubistSdkError::NoArtifactsDir(artifacts_dir, Box::new(e)))?;

        let mut result = HashMap::new();
        for json_file in read_dir_result
            .flat_map(|ent| ent.ok())
            .filter(|ent| ent.file_name().to_string_lossy().ends_with(".json"))
        {
            let (abi, bytes) = parse_contract(&json_file.path())?;
            let name = json_file
                .path()
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string();
            result.insert(
                name.clone(),
                ContractInfo {
                    fqn: ContractFQN::new(source_file.to_path_buf(), name),
                    data: ContractData::SolidityData { abi, bytes },
                },
            );
        }

        Ok(result)
    }
}

fn configure_solc_project(compiler_config: &CompilerConfig, paths: &TargetPaths) -> Project {
    // Configure compiler project. We only use the ethers_solc to compile a subset of contracts so
    // we scope the config to the solc directory in the build dir.
    let project_paths = ProjectPathsConfig {
        root: paths.build_root.clone(),
        cache: paths.compiler_cache.clone(),
        artifacts: paths.compiler_artifacts.clone(),
        build_infos: paths.compiler_build_infos.clone(),
        // We always call solc with explicit files; the project config should never look at
        // sources, tests, or scripts automatically. Since we can't easily enforce this we just
        // configure the path with bad directories.
        sources: paths.build_root.join("unexpected-use-sources"),
        tests: paths.build_root.join("unexpected-use-tests"),
        scripts: paths.build_root.join("unexpected-use-scripts"),
        libraries: vec![],
        remappings: vec![Remapping {
            name: ":stellar://".to_string(),
            path: paths
                .build_root
                .clone()
                .join("contracts")
                .join("stellar")
                .join("target")
                .join("wasm32-unknown-unknown")
                .join("release")
                .display()
                .to_string(),
        }],
    };
    Project::builder()
        .paths(project_paths)
        .set_cached(true)
        .set_build_info(true)
        .include_paths(compiler_config.import_dirs.clone())
        .build()
        .or_bug("Configuring 'solc' failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn simple_compile_ok() {
        let tmp = tempdir().unwrap();
        let contract_path = tmp.path().join("Test.sol");
        fs::write(
            &contract_path,
            r#"
            contract Store {
                uint256 number;
                function store(uint256 num) public {
                    number = num;
                }
            }
            "#,
        )
        .unwrap();

        let comp = SolcCompiler::new(
            &Default::default(),
            &TargetPaths::new(tmp.path().to_path_buf(), tmp.path().to_path_buf()),
        );
        let result = comp.compile_file(&contract_path).unwrap();
        assert_ne!("", result.diagnostics); // there should be some warnings in the diagnostics
    }

    #[test]
    fn simple_compile_fail() {
        let tmp = tempdir().unwrap();
        let contract_path = tmp.path().join("Test.sol");
        fs::write(&contract_path, "contract {").unwrap();
        let comp = SolcCompiler::new(
            &Default::default(),
            &TargetPaths::new(tmp.path().to_path_buf(), tmp.path().to_path_buf()),
        );
        match comp.compile_file(&contract_path) {
            Ok(result) => panic!(
                "Should have failed; instead succeeded with diagnostics: {}",
                result.diagnostics
            ),
            Err(CubistSdkError::CompileError(file, message, _)) => {
                assert_eq!(contract_path, file);
                assert!(message.contains("Expected identifier"));
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }
}
