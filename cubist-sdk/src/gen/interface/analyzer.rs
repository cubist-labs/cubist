//! Determine which functions and contracts are actually used cross-chain
use crate::analyze::visit::{walk_contract_definition, walk_expression, Visitor};
use crate::gen::common::{InterfaceGenError, Result};
use crate::parse::source_file::{SourceFile, SourceFileContent};
use cubist_config::util::OrBug;
use cubist_config::{ContractFile, ContractName, FunctionName, ObjectName, Target};
use solang_parser::pt;
use std::collections::{BTreeMap as Map, BTreeSet as OrdSet, HashSet as Set};
use std::path::{Path, PathBuf};
use tracing::warn;

/// Data structure that determines which functions and contracts to
/// create cross-contract interfaces for.
#[derive(Debug)]
pub struct Analyzer {
    /// Map of contract name to the target chain on which it runs
    contracts: Map<ContractName, Target>,
    /// Current contract that we are analyzing
    current_contract: Option<ContractName>,
    /// The target chain on which the current contract will be deployed
    current_target: Option<Target>,
    /// The file in which the current contract appears
    current_file: Option<ContractFile>,
    /// The map of cross-target objects. For example:
    ///   contract AvaStorage { // on Avalanche
    ///     EthStorage ethStorage; // on Ethereum
    ///     ...
    ///   }
    ///
    /// The above would yield:
    /// <AvaStorage, <ethStorage: EthStorage>>
    cross_target_objs: Map<ContractName, Map<ObjectName, ContractName>>,
    /// The list of contract functions that are used cross-chain
    cross_target_calls: Map<ContractName, OrdSet<FunctionName>>,
    /// Maps contracts to contract files
    contract_files: Map<ContractName, ContractFile>,
    /// The set of targets for each interface. For example, if an Ethereum contract "eth" is called
    /// from both Avalanche and Polygon, the map is { "eth.sol" -> [ Avalanche, Polygon ] }
    interface_targets: Map<ContractFile, Set<(ContractFile, Target)>>,
    /// Information about aliases as a result of renaming imports.
    /// For example, the following in AvaStorage.sol:
    /// import { EthereumStorage as EthStorage } from EthStorage.sol;
    ///
    /// Would yield:
    /// <AvaStorage.sol <EthStorage, EthereumStorage>>
    aliases: Map<ContractFile, Map<ObjectName, ObjectName>>,
}

impl Analyzer {
    /// Make a new blank analyzer
    pub fn new() -> Self {
        Analyzer {
            contracts: Map::new(),
            current_contract: None,
            current_target: None,
            current_file: None,
            cross_target_objs: Map::new(),
            cross_target_calls: Map::new(),
            contract_files: Map::new(),
            interface_targets: Map::new(),
            aliases: Map::new(),
        }
    }

    /// Get the reverse map of contract_files:
    /// an association between file paths and all the contracts they contain
    pub fn get_file_contracts(&self) -> Map<PathBuf, Vec<ContractName>> {
        let mut contracts = Map::<PathBuf, Vec<ContractName>>::new();
        for (contract_name, contract_file) in &self.contract_files {
            match contracts.get_mut(contract_file) {
                Some(vec) => vec.push(contract_name.to_string()),
                None => {
                    contracts.insert(contract_file.clone(), vec![contract_name.to_string()]);
                }
            }
        }
        contracts
    }

    /// Return all cross-chain contract dependencies.
    ///
    /// An entry like "A -> {B, C}" means that contract `A` may call
    /// contracts `B` and `C` (where both `B` and `C` are targeting a
    /// chain different from the target chain of contract `A`)
    pub fn get_cross_chain_dependencies(&self) -> Map<ContractName, Set<ContractName>> {
        self.cross_target_objs
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.values().map(Clone::clone).collect::<Set<ContractName>>(),
                )
            })
            .collect()
    }

    /// Return the association between contracts and the files in which they reside
    pub fn get_contract_files(&self) -> &Map<ContractName, PathBuf> {
        &self.contract_files
    }

    /// Get the set of all cross-target calls made within each contract
    pub fn get_call_info(&self) -> &Map<ContractName, OrdSet<FunctionName>> {
        &self.cross_target_calls
    }

    /// Get the set of targets for each interface that will be generated
    pub fn get_target_info(&self) -> &Map<ContractFile, Set<(ContractFile, Target)>> {
        &self.interface_targets
    }

    /// Determine which contracts reside in which files
    pub fn analyze_contract_locations(&mut self, sources: &[SourceFile]) -> Result<()> {
        for source in sources {
            // Save the location of the contract
            for name in &source.contract_names() {
                tracing::debug!("Analyzing {} in {}", name, source.file_name.display());
                if self
                    .contracts
                    .insert(name.to_string(), source.target)
                    .is_some()
                {
                    return Err(InterfaceGenError::DuplicateContracts(name.clone()));
                }
                self.contract_files
                    .insert(name.clone(), source.file_name.clone());
            }
        }
        Ok(())
    }

    /// Determine which contracts and functions are cross-chain
    pub fn analyze(&mut self, sources: &[SourceFile]) -> Result<()> {
        // One pass to save target chains for each file. We need this information in order to
        // determine which contract files and functions to create cross-chain interfaces for
        self.analyze_contract_locations(sources)?;

        // One pass to resolve aliasing information. We have to run this after the prior pass
        // because it relies on knowing contract-to-file associations
        sources.iter().for_each(|source| {
            source
                .import_directives()
                .iter()
                .for_each(|imp| self.add_aliases(source.file_name.as_path(), imp))
        });

        // One pass to determine which contracts' functions are cross-chain
        // This calls functions in the Analyzer's Visitor trait implementation,
        // which is at the bottom of this file
        for source in sources.iter() {
            match &source.content {
                SourceFileContent::SolidityContent { pt, .. } => self.visit_source_unit(pt)?,
                SourceFileContent::SorobanContent { .. } => {
                    warn!("Cannot determine cross-chain calls from Soroban contracts")
                }
            }
        }
        Ok(())
    }

    /// STEP ZERO:
    /// Identify aliases caused by renaming imports.
    /// This is necessary in order to identify cross-chain functions in later steps.
    /// For example, if contract EthereumStorage on Ethereum is renamed-by-import in
    /// AvaStorage.sol to EthStorage, we need to know that uses of EthStorage in
    /// AvaStorage.sol is cross-target (aka that EthStorage is on Ethereum).
    fn add_aliases(&mut self, source_file: &Path, imp: &pt::Import) {
        if let pt::Import::Rename(_, renamings, _) = imp {
            for (name, maybe_rename) in renamings {
                if let Some(rename) = maybe_rename {
                    self.aliases
                        .entry(source_file.to_path_buf())
                        .or_default()
                        .insert(rename.name.clone(), name.name.clone());
                }
            }
        }
    }

    /// STEP ONE:
    /// Identify which contract files are actually used cross-chain.
    /// This is necessary in order to identify cross-target functions.
    /// For example, to determine that ``ethStorage.store(5)'' is a
    /// cross-chain call, we first need to know that ethStorage lives
    /// on a different chain from the current contract
    ///
    /// This step also notes targets for cross-chain interfaces
    /// For example, encountering this code:
    ///   contract AvaStorage { // on Avalanche
    ///     ethStorage EthStorage; // on Ethereum
    ///     ...
    ///   }
    /// Will update the analyzer to say that there needs to be an Avalanche
    /// cross-chain interface for the contract EthStorage.
    fn id_cross_target_objs(&mut self, cd: &pt::ContractDefinition) -> Result<()> {
        for cp in &cd.parts {
            if let pt::ContractPart::VariableDefinition(def) = cp {
                let maybe_contract_name = match &def.ty {
                    pt::Expression::Variable(id) => Some(self.get_contract_alias(&id.name).clone()),
                    // This is matching on a qualified name, like Eth.EthStorage.
                    // Since the analyzer already checks that no contract name is used
                    // twice, we can ignore the Eth prefix: all we care about is whether
                    // or not EthStorage is a cross-chain contract
                    pt::Expression::MemberAccess(_, _, field) => {
                        let name = self.get_contract_alias(&field.name).clone();
                        // Is the name actually a contract?
                        // This is important because the match will also fire on qualified
                        // uses of top-level types (e.g., Eth.Integer if Integer is an
                        // enum defined outside of any contract in Eth).
                        // Since get_target returns the current target if it can't find
                        // a given contract, the below logic is *mostly* correct, unless
                        // a top-level type has the same name as some other contract.
                        self.contracts.contains_key(&name).then_some(name)
                    }
                    _ => None,
                };
                if let Some(contract_name) = maybe_contract_name {
                    let object_name = &def.name.name;
                    // Does the object live on a different chain than the current contract?
                    let object_target = self.get_target(&contract_name);
                    let current_target = self.get_current_target();
                    if current_target != object_target {
                        self.add_cross_target_obj(
                            object_name.to_string(),
                            contract_name.to_string(),
                        );
                        // Note the contract name should be in our map of contracts to files
                        let contract_file = self.contract_files[&contract_name].clone();
                        self.add_interface_target(
                            contract_file,
                            current_target,
                            self.current_file.clone().unwrap(),
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    /// STEP TWO:
    /// Using the information we gathered in step one, determine which functions
    /// cross two different blockchains. Save those functions and their contracts
    /// in cross_target_calls.
    fn id_cross_target_calls(&mut self, expr: &pt::Expression) -> Result<()> {
        if let pt::Expression::FunctionCall(_, fun, _) = expr {
            let member_access = match &**fun {
                // corresponds to calls like: member.fun(args)
                mem @ pt::Expression::MemberAccess(..) => Some(mem),
                // corresponds to calls like: member.fun{}(args)
                pt::Expression::FunctionCallBlock(_, expr, _) => Some(&**expr),
                _ => None,
            };
            if let Some(pt::Expression::MemberAccess(_, base, call)) = member_access {
                if let pt::Expression::Variable(id) = &**base {
                    let object_name = &id.name;
                    let call_name = &call.name;
                    let cc = self.get_current_contract();
                    // Which cross-chain contract, if any, does the object live on?
                    if let Some(other_contract) =
                        self.get_cross_target_obj_location(cc, object_name)
                    {
                        let other_contract_str = other_contract.to_string();
                        let call_name_str = call_name.to_string();
                        self.add_cross_target_call(other_contract_str, call_name_str);
                    }
                }
            }
        }
        Ok(())
    }

    /// If the contract name is an alias in the current file, return the contract it aliases
    /// Otherwise return `cn` itself.
    /// For example, if AvaStorage.sol contains:
    /// ```solidity
    /// import { EthereumStorage as EthStorage } from EthStorage.sol;
    /// // ...
    /// EthStorage ethStorage
    /// ```
    ///
    /// `get_contract_alias(EthStorage)` will return EthereumStorage
    fn get_contract_alias<'a>(&'a self, cn: &'a ContractName) -> &'a ContractName {
        let current_file = self
            .current_file
            .as_ref()
            .or_bug("get_contract_aliases expected a current file");
        if let Some(aliases) = self.aliases.get(current_file) {
            return aliases.get(cn).unwrap_or(cn);
        }
        cn
    }

    /// Get current contract name. Panics if the current contract is none
    fn get_current_contract(&self) -> &ContractName {
        self.current_contract
            .as_ref()
            .expect("get_current_contract when current contract is none")
    }

    /// Set the current contract (and update the current target)
    fn set_current_contract(&mut self, contract: ContractName) {
        if !self.cross_target_objs.contains_key(&contract) {
            self.cross_target_objs
                .insert(contract.to_string(), Map::new());
        }
        let target = self.get_target(&contract);
        let file = self.get_file(&contract);
        self.current_file = Some(file.to_path_buf());
        self.current_target = Some(target);
        self.current_contract = Some(contract);
    }

    /// Get current target. Panics if the target is none
    fn get_current_target(&self) -> Target {
        self.current_target
            .expect("get_current_target when current target is none")
    }

    /// Get the target that {cn} runs on
    /// We now enforce that every contract (with source code available, e.g.,
    /// not being pulled from Github) has a target. This function therefore
    /// assumes that any contract without an explicit target is running on
    /// the same chain as the current contract.
    fn get_target(&self, cn: &ContractName) -> Target {
        // We have an explicit target for the contract:
        // It was defined in the source files listed in the config
        self.contracts
            .get(cn)
            .copied()
            .or(self.current_target) // Must be an external import
            .or_bug("get_target expects a current target chain")
    }

    /// Get the file in which `cn` lives
    fn get_file(&self, cn: &ContractName) -> &ContractFile {
        self.contract_files
            .get(cn)
            .or_bug("get_file for contract with no file")
    }

    /// Given an object `obj` in the contract `contract`,
    /// determine which contract `obj` lives in
    /// e.g., the ethStorage object lives on the EthStorage contract
    fn get_cross_target_obj_location(
        &self,
        contract: &ContractName,
        obj: &ObjectName,
    ) -> Option<&ContractName> {
        self.cross_target_objs
            .get(contract)
            .or_bug("get_obj_target for object in non existent contract")
            .get(obj)
    }

    /// Add <`obj`, `contract`> to the map of the current contract's cross target objects
    fn add_cross_target_obj(&mut self, obj: ObjectName, contract: ContractName) {
        let cc = self.get_current_contract().to_string();
        if let Some(objs) = self.cross_target_objs.get_mut(&cc) {
            objs.insert(obj, contract);
        } else {
            self.cross_target_objs
                .insert(cc, Map::from([(obj, contract)]));
        }
    }

    /// Note that `contract`'s `function` is used cross-chain
    fn add_cross_target_call(&mut self, contract: ContractName, function: FunctionName) {
        if let Some(functions) = self.cross_target_calls.get_mut(&contract) {
            functions.insert(function);
        } else {
            self.cross_target_calls
                .insert(contract, OrdSet::from([function]));
        }
    }

    /// Adds `target` to the `source`'s set of targets
    fn add_interface_target(
        &mut self,
        source: ContractFile,
        target: Target,
        sender_name: ContractFile,
    ) -> Result<()> {
        self.interface_targets
            .entry(source)
            .or_default()
            .insert((sender_name, target));
        Ok(())
    }
}

/// Actually call the analysis routines defined above
impl Visitor for Analyzer {
    /// Identify cross-target objects
    fn visit_contract_definition(&mut self, cd: &pt::ContractDefinition) -> Result<()> {
        self.set_current_contract(cd.name.name.to_string());
        self.id_cross_target_objs(cd)?;
        walk_contract_definition(self, cd)
    }

    /// Identify cross-target calls
    fn visit_expression(&mut self, expr: &pt::Expression) -> Result<()> {
        self.id_cross_target_calls(expr)?;
        walk_expression(self, expr)
    }
}
