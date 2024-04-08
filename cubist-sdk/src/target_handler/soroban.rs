use std::collections::HashMap;
use std::fs;
use std::path::Path;

use convert_case::{Case, Casing};
use cubist_config::paths::{ContractFQN, TargetPaths};
use sha2::{Digest, Sha256};

use crate::core::{CompileResult, ContractCompiler, ContractData, ContractInfo};
use crate::Result;

pub struct SorobanCompiler {
    /// Per-target well-known paths
    pub target_paths: TargetPaths,
}

impl SorobanCompiler {
    pub fn new(target_paths: TargetPaths) -> Self {
        Self { target_paths }
    }
}

impl ContractCompiler for SorobanCompiler {
    fn clean(&self) -> Result<()> {
        todo!()
    }

    fn compile_file(&self, _file: &Path) -> Result<CompileResult> {
        todo!()
    }

    fn find_compiled_contracts(&self, source_file: &Path) -> Result<HashMap<String, ContractInfo>> {
        // TODO: make nicer
        let mut res = HashMap::new();
        let source_path = self.target_paths.contracts.join(source_file);
        let contract_name = source_file
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_case(Case::UpperCamel);
        let wasm = fs::read(&source_path).unwrap();
        let hash = format!("{:x}", Sha256::digest(wasm.clone()));
        let spec_entries = soroban_spec::read::from_wasm(&wasm).unwrap();
        res.insert(
            contract_name.to_string(),
            ContractInfo {
                fqn: ContractFQN::new(source_file.to_path_buf(), contract_name.to_string()),
                data: ContractData::SorobanData {
                    wasm_path: source_path.clone(),
                    spec_entries,
                    hash: hash.to_string(),
                },
            },
        );
        Ok(res)
    }
}
