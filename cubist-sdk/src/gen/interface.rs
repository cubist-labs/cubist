/// Generate cross-chain interfaces to expose contracts
/// on one chain to a different chain. Yields an out directory
/// that contains one file per cross-chain contract, and a bridge
/// file that describes how to bridge calls from one chain to another
mod analyzer;
mod config;
mod contract;
pub mod file;
mod import;
use crate::gen::common::{InterfaceGenError, Result};
use crate::parse::{source_file::SourceFile, SourceFiles};
use analyzer::Analyzer;
use config::{AnalysisInfo, ExplicitInfo, InterfaceConfig};
use cubist_config::{ContractName, Target};
use file::FileInterfaces;
use std::collections::{BTreeMap as Map, HashSet as Set};
use std::path::{Path, PathBuf};

/// Interface information used later, notably by the cubist-cli pre-compile command
pub struct Interfaces {
    /// Per-file interface information. A [`FileInterfaces`] represents all the    
    /// ContractInterfaces for a given file; many FileInterfaces correspond
    /// to many files. The [`FileInterfaces`] struct contains utilities for interacting   
    /// with interfaces (e.g., getting their code or bridging information).
    pub interfaces: Vec<FileInterfaces>,
    /// Information about which paths lead to files with which contracts
    /// (i.e., which contracts are in which files).
    pub contract_locations: Map<PathBuf, Vec<ContractName>>,
    /// Cross-chain contract dependecies.
    ///
    /// An entry like "A -> {B, C}" means that contract `A` may call
    /// contracts `B` and `C` (where both `B` and `C` are targeting a
    /// chain different from the target chain of contract `A`).
    pub cross_chain_deps: Map<ContractName, Set<ContractName>>,
}

impl Interfaces {
    /// Returns a list of contracts in a file, each mapped to other
    /// contracts (not necessarily in the same file) that it may call.
    pub fn get_contracts_in_file(&self, path: &Path) -> Map<ContractName, Set<ContractName>> {
        self.contract_locations
            .get(path)
            .unwrap_or(&vec![])
            .iter()
            .cloned()
            .map(|c| {
                let deps = self
                    .cross_chain_deps
                    .get(&c)
                    .map(Clone::clone)
                    .unwrap_or_default();
                (c, deps)
            })
            .collect()
    }
}

/// Returns interface information for everything specified in the config.
pub fn get_interfaces(source_files: &SourceFiles) -> Result<Interfaces> {
    let cross_chain_analyzer = analyze(source_files)?;
    let interface_config = InterfaceConfig::from(AnalysisInfo {
        included_code: cross_chain_analyzer.get_call_info().clone(),
        targets: cross_chain_analyzer.get_target_info().clone(),
    });
    let interfaces = to_file_interfaces(&interface_config, &source_files.sources)?;
    let contract_locations = cross_chain_analyzer.get_file_contracts();
    let cross_chain_deps = cross_chain_analyzer.get_cross_chain_dependencies();
    Ok(Interfaces {
        interfaces,
        contract_locations,
        cross_chain_deps,
    })
}

/// Returns interfaces for only the public, exposable functions in `contract`.
/// These interfaces will be deployed on `targets`. In contrast to [`get_interfaces`]
/// above, this function *does not* infer which contracts and functions to generate
/// interfaces for, nor does it infer the targets on which to deploy those interfaces.
/// That is why this function requires the `contract` and `targets` parameter.
pub fn get_interface_for_contract(
    source_files: &SourceFiles,
    contract: &ContractName,
    targets: &Set<Target>,
) -> Result<Interfaces> {
    // Error check and figure out which file contains contract
    if source_files.sources.is_empty() {
        return Err(InterfaceGenError::MissingContracts);
    }
    let mut contract_finder = Analyzer::new();
    contract_finder.analyze_contract_locations(&source_files.sources)?;
    let files = contract_finder.get_contract_files();
    if !files.contains_key(contract) {
        return Err(InterfaceGenError::MissingContract(contract.clone()));
    }
    // Generate the interfaces for contract
    let interface_config = InterfaceConfig::from(ExplicitInfo {
        contract: contract.clone(),
        file: files[contract].clone(),
        targets: targets.clone(),
    });
    let interfaces = to_file_interfaces(&interface_config, &source_files.sources)?;
    let contract_locations = contract_finder.get_file_contracts();
    let cross_chain_deps = contract_finder.get_cross_chain_dependencies();
    Ok(Interfaces {
        interfaces,
        contract_locations,
        cross_chain_deps,
    })
}

/// Construct [`SourceFile`]s and perform cross-chain analysis.
fn analyze(source_files: &SourceFiles) -> Result<Analyzer> {
    if source_files.sources.is_empty() {
        return Err(InterfaceGenError::MissingContracts);
    }

    // Since we're generating interfaces only for functions and contracts that
    // are used cross-chain, we need to run the analyzer to figure out
    // *which* functions and contracts are actually cross-chain
    let mut cross_chain_analyzer = Analyzer::new();
    cross_chain_analyzer.analyze(&source_files.sources)?;

    Ok(cross_chain_analyzer)
}

/// Convert source files and the result of a cross-chain analysis to a
/// vector of FileInterfaces. A FileInterface represents all the
/// ContractInterfaces for a given file; many FileInterfaces
/// correspond to many files. The FileInterface struct contains
/// utilities for interacting with interfaces (e.g., getting their
/// code or bridging information).
fn to_file_interfaces(
    interface_config: &InterfaceConfig,
    sources: &[SourceFile],
) -> Result<Vec<FileInterfaces>> {
    // Generate interfaces only for the cross-chain functions and contracts listed
    // in the interface_config we just made.
    sources
        .iter()
        // Only keep the source files that we generate an interface for
        .filter(|source| interface_config.requires_interface(&source.file_name))
        // Map each source file to a (result of) a list of file interfaces
        .map(|source| {
            let file_name = &source.file_name;
            interface_config
                .get_interface_targets(file_name)?
                .iter()
                .map(|target| FileInterfaces::new(source, interface_config, *target))
                .collect()
        })
        // Consolidate all results into a single result
        .collect::<Result<Vec<Vec<FileInterfaces>>>>()
        // If successful, flatten the list of lists of file interfaces into a single list of file
        // interfaces
        .map(|interfaces| {
            interfaces
                .into_iter()
                .flatten()
                .collect::<Vec<FileInterfaces>>()
        })
}
