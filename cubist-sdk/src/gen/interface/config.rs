//! Our internal configuration for describing the files and contracts and functions
//! for which to generate interfaces.
use crate::gen::common::{InterfaceGenError, Result};
use cubist_config::{ContractFile, ContractName, FunctionName, Target};
use std::collections::{BTreeMap as Map, BTreeSet as OrdSet, HashSet as Set};

/// Information the interface generator needs in order to generate cross-chain interfaces
#[derive(Debug)]
pub enum InterfaceConfig {
    /// Generate interfaces that expose every function in the given contract
    ExposedContract(ExplicitInfo),
    /// Generate interfaces based on analysis information about what's used
    /// cross-chain in a given project
    AnalyzedProject(AnalysisInfo),
}

/// Information about a single contract that we're explicitly exposing
#[derive(Debug)]
pub struct ExplicitInfo {
    /// The contract that we're explicitly exposing
    pub contract: ContractName,
    /// The file in which that contract resides
    pub file: ContractFile,
    /// The targets we're exposing the contract to
    pub targets: Set<Target>,
}

/// Analysis information about what's used cross-chain (and thus what requires an interface)
/// This is implicit and was determined by the analyzer
#[derive(Debug)]
pub struct AnalysisInfo {
    /// Contracts and their functions that should actually get interfaces
    pub included_code: Map<ContractName, OrdSet<FunctionName>>,
    /// Maps contract files to a set of targets
    pub targets: Map<ContractFile, Set<Target>>,
}

/// Make a new interface config given a contract and a file
impl From<ExplicitInfo> for InterfaceConfig {
    fn from(ei: ExplicitInfo) -> InterfaceConfig {
        InterfaceConfig::ExposedContract(ei)
    }
}

/// Make a new interface config given analysis information
impl From<AnalysisInfo> for InterfaceConfig {
    fn from(ai: AnalysisInfo) -> InterfaceConfig {
        InterfaceConfig::AnalyzedProject(ai)
    }
}

impl InterfaceConfig {
    pub fn expose_all(&self) -> bool {
        matches!(self, InterfaceConfig::ExposedContract(..))
    }

    /// Returns whether `contract` requires the generation of interface files
    pub fn requires_interface(&self, contract: &ContractFile) -> bool {
        match self {
            InterfaceConfig::ExposedContract(info) => contract == &info.file,
            InterfaceConfig::AnalyzedProject(info) => info.targets.contains_key(contract),
        }
    }

    /// Which targets should `contract`'s interface run on?
    pub fn get_interface_targets(&self, contract: &ContractFile) -> Result<&Set<Target>> {
        match self {
            InterfaceConfig::ExposedContract(info) => Ok(&info.targets),
            InterfaceConfig::AnalyzedProject(info) => info
                .targets
                .get(contract)
                .ok_or_else(|| InterfaceGenError::UnknownInterface(contract.display().to_string())),
        }
    }

    /// Generate an interface for this contract?
    pub fn gen_contract(&self, contract: &ContractName) -> bool {
        match self {
            InterfaceConfig::ExposedContract(info) => contract == &info.contract,
            InterfaceConfig::AnalyzedProject(info) => info.included_code.contains_key(contract),
        }
    }

    /// Generate an interface for this function?
    pub fn gen_function(&self, contract: &ContractName, function: &FunctionName) -> bool {
        match self {
            InterfaceConfig::ExposedContract(info) => contract == &info.contract,
            InterfaceConfig::AnalyzedProject(info) => info
                .included_code
                .get(contract)
                .map_or(false, |map| map.contains(function)),
        }
    }

    /// Does the map of functions to include contain something we didn't see
    /// in the contract? This can happen eg for automatically generated getters.
    /// These don't show up in the parse tree.
    /// If so, return the name of the first function that isn't in the parse tree
    pub fn missed_function(
        &self,
        contract: &ContractName,
        functions: &[&String],
    ) -> Option<String> {
        match self {
            InterfaceConfig::ExposedContract(..) => None,
            InterfaceConfig::AnalyzedProject(info) => {
                // Fix this monstrosity later
                if info.included_code.contains_key(contract) {
                    let included_functions = info.included_code.get(contract).unwrap();
                    for function in included_functions {
                        if !functions.contains(&function) {
                            return Some(function.clone());
                        }
                    }
                }
                None
            }
        }
    }
}
