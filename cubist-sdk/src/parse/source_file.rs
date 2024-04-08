/// Our representation of an source file.
/// We use this representation because we need to package a file's
/// contents and AST and target so that we can both analyze it and
/// later produce an interface for it and a bridge file. In other
/// words: we need this information packaged together multiple times
use crate::{
    gen::{
        common::InterfaceGenError,
        interface::{config::InterfaceConfig, contract::ContractInterface, file::Pragma},
    },
    CubistSdkError, Result,
};
use convert_case::{Case, Casing};
use cubist_config::{Config, ContractName, Target};
use cubist_util::fs::is_within;
use solang_parser::pt;
use soroban_env_host::xdr::ScSpecEntry;
use std::fs;
use std::path::{Path, PathBuf};

const STELLAR_IMPORT_PREFIX: &str = "stellar://";

/// A source file with its AST and additional meta information
// TODO: Make trait
#[derive(Debug)]
pub struct SourceFile {
    /// The source file's absolute path
    pub file_name: PathBuf,
    /// The source file's relative path to a root contracts folder
    pub rel_path: PathBuf,
    /// What target the code in the file runs on
    pub target: Target,
    /// Information related to the content of the file
    pub content: SourceFileContent,
}

/// Content of a source file
#[derive(Debug)]
pub enum SourceFileContent {
    /// Solidity content
    SolidityContent {
        /// The AST the file contains
        pt: pt::SourceUnit,
        /// Comments from the source file
        comments: Vec<pt::Comment>,
    },
    /// Soroban content
    SorobanContent {
        /// Specification entries of the source file
        spec_entries: Vec<ScSpecEntry>,
    },
}

impl SourceFile {
    /// Create a new source file given a Cubist contract config.
    /// Errors if it encounters problems with in file system or the parser.
    pub fn new(file: impl AsRef<Path>, rel_path: PathBuf, target: Target) -> Result<Self> {
        let file_name = file.as_ref().to_path_buf();

        let content = match target {
            Target::Stellar => {
                let wasm = fs::read(file).unwrap();
                let spec_entries = soroban_spec::read::from_wasm(&wasm).unwrap();
                SourceFileContent::SorobanContent { spec_entries }
            }
            _ => {
                let code = fs::read_to_string(file.as_ref())
                    .map_err(|e| CubistSdkError::ReadFileError(file.as_ref().into(), e))?;
                match solang_parser::parse(&code, 0) {
                    Ok((pt, comments)) => SourceFileContent::SolidityContent { pt, comments },
                    Err(es) => Err(CubistSdkError::ParseError(file_name.clone(), es))?,
                }
            }
        };

        Ok(SourceFile {
            file_name,
            rel_path,
            target,
            content,
        })
    }

    /// Returns a list of contracts in the file
    pub fn contract_names(&self) -> Vec<ContractName> {
        match &self.content {
            SourceFileContent::SolidityContent { pt, .. } => {
                pt.0.iter()
                    .filter_map(|part| match part {
                        pt::SourceUnitPart::ContractDefinition(cd) => Some(cd.name.name.clone()),
                        _ => None,
                    })
                    .collect()
            }
            SourceFileContent::SorobanContent { .. } => {
                vec![self
                    .file_name
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .to_case(Case::UpperCamel)]
            }
        }
    }

    /// Returns a list of import directives in the source file
    pub fn import_directives(&self) -> Vec<pt::Import> {
        match &self.content {
            SourceFileContent::SolidityContent { pt, .. } => {
                pt.0.iter()
                    .filter_map(|part| match part {
                        pt::SourceUnitPart::ImportDirective(imp) => Some(imp.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            }
            SourceFileContent::SorobanContent { .. } => vec![],
        }
    }

    /// Returns the license as a string if available (currently only Solidity)
    pub fn license(&self) -> Result<Option<String>, InterfaceGenError> {
        match &self.content {
            SourceFileContent::SolidityContent { comments, .. } => {
                if comments.is_empty() {
                    // No comment means no license
                    return Ok(None);
                } else {
                    // The license is supposed to be the first thing in the comments
                    let contents = comments[0].get_contents();
                    // Solidity will error if there are multiple licenses,
                    // so we don't check that case: compiling the original
                    // contract at the next stage will result in an error
                    if let Some((_, after)) = contents.split_once("SPDX-License-Identifier:") {
                        // There has to be *some* license after the license identifier
                        if after.is_empty() {
                            return Err(InterfaceGenError::MissingLicense);
                        }
                        // Find the first thing after the license identifier
                        let mut license = None;
                        for word in after.split(' ').collect::<Vec<&str>>() {
                            if !word.is_empty() {
                                license = Some(word.to_string());
                                break;
                            }
                        }
                        if license.is_none() {
                            return Err(InterfaceGenError::MissingLicense);
                        }
                        return Ok(license);
                    }
                }
                Ok(None)
            }
            SourceFileContent::SorobanContent { .. } => Ok(None),
        }
    }

    /// Returns the pragmas in the file
    pub fn pragmas(&self) -> Vec<Pragma> {
        match &self.content {
            SourceFileContent::SolidityContent { pt, .. } => {
                let mut result = Vec::new();
                for part in &pt.0 {
                    if let pt::SourceUnitPart::PragmaDirective(..) = part {
                        result.push(Pragma(part.clone()));
                    }
                }
                result
            }
            SourceFileContent::SorobanContent { .. } => vec![],
        }
    }

    /// Returns the contract interfaces in the file
    pub fn interfaces(
        &self,
        config: &InterfaceConfig,
    ) -> Result<Vec<ContractInterface>, InterfaceGenError> {
        match &self.content {
            SourceFileContent::SolidityContent { pt, .. } => {
                let mut result = Vec::new();
                for part in &pt.0 {
                    if let pt::SourceUnitPart::ContractDefinition(cd) = part {
                        let contract_name = &cd.name.name;
                        // Are we supposed to generate an interface for this contract?
                        if !config.gen_contract(contract_name) {
                            continue;
                        }
                        let interface = ContractInterface::new(config, cd)?;
                        result.push(interface);
                    }
                }
                Ok(result)
            }
            SourceFileContent::SorobanContent { spec_entries } => {
                let name = self.contract_names().first().unwrap().clone();
                Ok(vec![ContractInterface::from_soroban_spec(
                    &name,
                    spec_entries,
                )])
            }
        }
    }

    /// Check the imports in this file for:
    /// (1) Absolute paths that point into the contracts root directory.
    ///     This is a problem since Cubist copies the contents of the contracts dir.
    /// (2) Relative paths that point outside the contracts root directory.
    ///     This is a problem for the same reason as the previous.
    pub fn check_imports(&self, config: &Config) -> Result<()> {
        let root_dir = &config.contracts().root_dir;
        for import in self.import_directives() {
            let import_lit = match import {
                pt::Import::Plain(s, ..) => s,
                pt::Import::GlobalSymbol(s, ..) => s,
                pt::Import::Rename(s, ..) => s,
            };
            // Check for external (node) imports. Those are fine.
            // In the future we'll need to extend this for other external imports we support
            if import_lit.string.starts_with('@') {
                continue;
            }
            // Ignore imports of Stellar contracts
            if import_lit.string.starts_with(STELLAR_IMPORT_PREFIX) {
                continue;
            }
            if import_lit.unicode {
                return Err(CubistSdkError::UnicodeImportError(
                    import_lit.string,
                    self.file_name.clone(),
                ));
            }
            let path = Path::new(&import_lit.string);
            // Is it an absolute path that points into the root directory?
            if path.is_absolute() {
                match is_within(path, root_dir) {
                    Err(e) => {
                        return Err(CubistSdkError::CanonicalizationError(
                            import_lit.string,
                            self.file_name.clone(),
                            Some(e),
                        ))
                    }
                    Ok(points_into_root) => {
                        if points_into_root {
                            return Err(CubistSdkError::AbsolutePathError(
                                import_lit.string,
                                self.file_name.clone(),
                            ));
                        }
                    }
                }
            }
            // Is it a relative path that points outside the root directory?
            if path.is_relative() {
                let parent_path = self.file_name.parent();
                if parent_path.is_none() {
                    return Err(CubistSdkError::CanonicalizationError(
                        import_lit.string,
                        self.file_name.clone(),
                        None,
                    ));
                }
                let full_path = parent_path.unwrap().join(path);
                match is_within(&full_path, root_dir) {
                    Err(e) => {
                        return Err(CubistSdkError::CanonicalizationError(
                            import_lit.string,
                            self.file_name.clone(),
                            Some(e),
                        ))
                    }
                    Ok(is_within_root) => {
                        if !is_within_root {
                            return Err(CubistSdkError::RelativePathError(
                                import_lit.string,
                                self.file_name.clone(),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
