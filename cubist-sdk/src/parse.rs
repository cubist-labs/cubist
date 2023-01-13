/// Module for the representation of source files and their AST
pub mod source_file;

use crate::Result;
use cubist_config::ContractsConfig;
use solang_parser::pt;
use source_file::SourceFile;

/// A collection of source files with their ASTs and other information
pub struct SourceFiles {
    /// The source files (absolute paths)
    pub sources: Vec<SourceFile>,
}

impl SourceFiles {
    /// Parses the files
    pub fn new(config: &ContractsConfig) -> Result<Self> {
        let sources = config
            .targets
            .iter()
            .flat_map(|(target, target_config)| {
                target_config.files().iter().map(|file| {
                    SourceFile::new(file, config.relative_to_root(file).unwrap(), *target)
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(SourceFiles { sources })
    }

    /// Returns the number of source files.
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// Returns true if `len()` is 0.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Returns the path part of an import (i.e., the part that refers to the actual file to import).
pub fn get_import_path(import: &pt::Import) -> &String {
    match import {
        pt::Import::Plain(s, ..) => &s.string,
        pt::Import::GlobalSymbol(s, ..) => &s.string,
        pt::Import::Rename(s, ..) => &s.string,
    }
}
